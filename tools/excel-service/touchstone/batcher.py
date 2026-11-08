"""Work queues and batch assembly. Pure logic: no I/O, no Excel, no DnaCalc imports.

Calls arrive as pieces (a few groups each).  A piece is cut into work items of at most CHUNK calls;
items wait in per-origin queues, one set for interactive work and one for bulk.  A freed worker takes
interactive items first (all of them, nothing else in that batch, so the batch is short), otherwise bulk
items by deficit round robin across origins.  Items with the same function and argument shape are merged
into one group, so a batch costs one formula assignment per shape.
"""
from collections import deque
import time

import protocol

CHUNK = 5000                 # bulk is cut into items of at most this many calls
BATCH_CALLS = 20000          # most calls in one batch when the queue is deep
BATCH_BYTES = 4 * 1024 * 1024
QUANTUM = 5000               # deficit added to an origin each round
INTERACTIVE_CALLS = 100      # requests at most this big are interactive unless flagged otherwise
APPROX_BYTES_PER_VALUE = 18
BULK_BATCH_MS = 120.0         # a bulk batch is cut to about this much estimated work, so interactive calls wait little
MIN_BULK_CALLS = 2000         # never cut a bulk batch below this many calls (fixed costs per batch)
MIN_LEARN_CALLS = 200
BATCH_FIXED_MS = 4.0          # fixed cost per batch, and per distinct shape below
SHAPE_FIXED_MS = 1.0
DEFAULT_COST_US = 25.0        # per call, until a function has been measured


class Piece:
    """One submit frame's worth of calls: answered as a unit by (origin, id, seq)."""
    __slots__ = ("conn", "origin", "rid", "seq", "last", "priority", "groups", "calls", "echo", "serial",
                 "t_received", "t_dispatched", "remaining", "frames", "done", "fail", "env_ids", "workers")

    def __init__(self, conn, rid, seq, groups, last, priority, echo=False, serial=False):
        self.conn, self.origin, self.rid, self.seq, self.last = conn, conn.origin, rid, seq, last
        self.priority = priority
        self.groups = groups
        self.calls = sum(g["n"] for g in groups)
        self.echo, self.serial = echo, serial
        self.t_received = time.monotonic()
        self.t_dispatched = None
        self.remaining = 0
        self.frames = []        # answer frames produced so far (kept for replay)
        self.done = False
        self.fail = None
        self.env_ids = set()
        self.workers = set()


class Item:
    """A slice of one group of one piece, at most CHUNK calls."""
    __slots__ = ("piece", "group", "n")

    def __init__(self, piece, group):
        self.piece, self.group, self.n = piece, group, group["n"]


class Slice:
    """Where an item's calls sit inside a merged group of a batch."""
    __slots__ = ("item", "merged", "offset")

    def __init__(self, item, merged, offset):
        self.item, self.merged, self.offset = item, merged, offset


class Batch:
    __slots__ = ("groups", "slices", "calls", "echo", "serial", "t_created", "pieces", "interactive")

    def __init__(self):
        self.groups, self.slices, self.calls = [], [], 0
        self.echo = self.serial = False
        self.t_created = time.monotonic()
        self.pieces = set()
        self.interactive = False


def shape_key(group):
    return (group["f"], group["k"], tuple(group["x"]) if "x" in group else None)


class _Origin:
    """One origin's queued items, grouped by shape in first-arrival order (a batch then holds few shapes)."""
    __slots__ = ("shapes",)

    def __init__(self):
        self.shapes = {}          # dict keeps insertion order: shape key -> deque of Item

    def empty(self):
        return not self.shapes

    def first(self):
        key = next(iter(self.shapes))
        return key, self.shapes[key]


class _Class:
    """Per-class queues with deficit round robin over origins."""

    def __init__(self):
        self.origins = {}
        self.order = deque()             # origins with queued work
        self.deficit = {}
        self.calls = 0

    def add(self, item):
        origin = item.piece.origin
        o = self.origins.get(origin)
        if o is None:
            o = self.origins[origin] = _Origin()
        if o.empty():
            self.order.append(origin)
            self.deficit.setdefault(origin, 0)
        o.shapes.setdefault(shape_key(item.group), deque()).append(item)
        self.calls += item.n

    def push_front(self, item):
        origin = item.piece.origin
        o = self.origins.setdefault(origin, _Origin())
        if o.empty():
            self.order.appendleft(origin)
            self.deficit.setdefault(origin, 0)
        key = shape_key(item.group)
        dq = o.shapes.pop(key, None) or deque()
        dq.appendleft(item)
        # re-insert this shape first
        o.shapes = {key: dq, **o.shapes}
        self.calls += item.n

    def empty(self):
        return not self.order

    def take(self, budget_calls, budget_bytes, isolate, costs=None, budget_ms=None):
        """Items by DRR until a budget is spent; an item of an isolated function goes alone."""
        taken, calls, size, spent = [], 0, 0, 0.0
        while self.order and calls < budget_calls and size < budget_bytes:
            origin = self.order[0]
            o = self.origins[origin]
            self.deficit[origin] += QUANTUM
            stop = False
            while not o.empty() and self.deficit[origin] > 0 and calls < budget_calls and size < budget_bytes:
                key, dq = o.first()
                item = dq[0]
                if item.group["f"] in isolate:
                    # A function that crashed Excel twice runs as a one-call batch of its own.
                    if taken:
                        stop = True
                        break
                    if item.n > 1:
                        head, tail = split_item(item, 1)
                        dq[0] = tail
                    else:
                        head = dq.popleft()
                        if not dq:
                            del o.shapes[key]
                    self.calls -= 1
                    if o.empty():
                        self.order.popleft()
                        self.deficit[origin] = 0
                    return [head], 1, 100
                room = budget_calls - calls
                if budget_ms is not None:
                    cost = costs.get(item.group["f"], DEFAULT_COST_US)
                    room = min(room, max(MIN_BULK_CALLS, int((budget_ms - spent) * 1000 / cost)))
                if item.n > room:
                    if calls and room < item.n // 2:
                        stop = True      # the rest waits for the next batch; no progress is possible here
                        break
                    head, tail = split_item(item, room)
                    dq[0] = tail
                    item = head
                else:
                    dq.popleft()
                    if not dq:
                        del o.shapes[key]
                taken.append(item)
                calls += item.n
                if budget_ms is not None:
                    spent += item.n * costs.get(item.group["f"], DEFAULT_COST_US) / 1000
                    if spent >= budget_ms:
                        stop = True
                size += item.n * max(1, protocol.group_width(item.group)) * APPROX_BYTES_PER_VALUE
                self.calls -= item.n
                self.deficit[origin] -= item.n
                if stop:
                    break
            self.order.popleft()
            if not o.empty():
                self.order.append(origin)
            else:
                self.deficit[origin] = 0
            if stop:
                break
        return taken, calls, size


def split_item(item, head_calls):
    """Split an item into (first head_calls calls, rest); both belong to the same piece."""
    group = item.group
    width = protocol.group_width(group)
    head = {"f": group["f"], "k": group["k"], "n": head_calls, "v": group["v"][:head_calls * width]}
    tail = {"f": group["f"], "k": group["k"], "n": group["n"] - head_calls, "v": group["v"][head_calls * width:]}
    for g in (head, tail):
        if "x" in group:
            g["x"] = group["x"]
    if "o" in group:
        head["o"] = group["o"]
        tail["o"] = group["o"] + head_calls
    else:
        head["i"] = group["i"][:head_calls]
        tail["i"] = group["i"][head_calls:]
    piece = item.piece
    piece.remaining += 1
    return Item(piece, head), Item(piece, tail)


class Batcher:
    def __init__(self):
        self.interactive = _Class()
        self.bulk = _Class()
        self.isolate = set()
        self.recent_interactive = 0.0
        self.costs = {}          # function -> estimated microseconds per call

    def queued_calls(self):
        return self.interactive.calls + self.bulk.calls

    def add_piece(self, piece):
        cls = self.interactive if piece.priority == "interactive" else self.bulk
        if piece.priority == "interactive":
            self.recent_interactive = time.monotonic()
        items = 0
        for group in piece.groups:
            for part in protocol.split_group(group, CHUNK):
                cls.add(Item(piece, part))
                items += 1
        piece.remaining = items

    def has_work(self):
        return not (self.interactive.empty() and self.bulk.empty())

    def next_batch(self):
        """Assemble the next batch for an idle worker, or None."""
        if not self.has_work():
            return None
        batch = Batch()
        if not self.interactive.empty():
            items, calls, _ = self.interactive.take(BATCH_CALLS, BATCH_BYTES, self.isolate)
            batch.interactive = True
        else:
            # A deep backlog and no interactive traffic lately: use the large batch, else keep batches short.
            deep = self.bulk.calls > 4 * BATCH_CALLS and time.monotonic() - self.recent_interactive > 2.0
            items, calls, _ = self.bulk.take(BATCH_CALLS if deep else 2 * CHUNK, BATCH_BYTES, self.isolate,
                                             self.costs, BULK_BATCH_MS * (3 if deep else 1))
        if not items:
            return None
        merged = {}
        for item in items:
            g = item.group
            key = (g["f"], g["k"], tuple(g["x"]) if "x" in g else None)
            entry = merged.get(key)
            if entry is None:
                m = {"f": g["f"], "k": g["k"], "n": 0, "v": []}
                if "x" in g:
                    m["x"] = g["x"]
                batch.groups.append(m)
                entry = merged[key] = (m, len(batch.groups) - 1)
            m, index = entry
            batch.slices.append(Slice(item, index, m["n"]))
            m["n"] += g["n"]
            m["v"].extend(g["v"])
            batch.pieces.add(item.piece)
            batch.echo = batch.echo or item.piece.echo
            batch.serial = batch.serial or item.piece.serial
        batch.calls = calls
        now = time.monotonic()
        for piece in batch.pieces:
            if piece.t_dispatched is None:
                piece.t_dispatched = now
        return batch

    def learn(self, batch, actual_ms):
        """Scale the per-function cost estimates of a finished batch by how far off the prediction was."""
        if batch.calls < MIN_LEARN_CALLS:
            return           # fixed costs would be charged to the few calls
        fixed = SHAPE_FIXED_MS * len(batch.groups)
        actual_ms = max(actual_ms - fixed, 0.2 * actual_ms)
        predicted = sum(g["n"] * self.costs.get(g["f"], DEFAULT_COST_US) for g in batch.groups) / 1000
        if predicted <= 0 or actual_ms <= 0:
            return
        ratio = max(0.25, min(4.0, actual_ms / predicted))
        for g in batch.groups:
            old = self.costs.get(g["f"], DEFAULT_COST_US)
            self.costs[g["f"]] = min(2000.0, max(0.5, old * ratio ** 0.5 if g["f"] in self.costs else old * ratio))

    def requeue(self, batch):
        """Put a batch's items back at the front (a worker was lost before running it)."""
        for sl in reversed(batch.slices):
            cls = self.interactive if sl.item.piece.priority == "interactive" else self.bulk
            cls.push_front(sl.item)
