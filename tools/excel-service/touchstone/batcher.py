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
QUANTUM_US = 40_000          # deficit (estimated microseconds of Excel time) added to a client each round
MAX_SPILL_CALLS = 1000       # a spill batch lays one anchor per call on a grid; keep the grid small
INTERACTIVE_CALLS = 100      # requests at most this big are interactive unless flagged otherwise
APPROX_BYTES_PER_VALUE = 18
BULK_BATCH_MS = 800.0         # a bulk batch is cut to about this much estimated work (a batch also costs ~150 ms of fixed
                              # fingerprint and COM time, measured under load, so short batches would mostly pay that;
                              # interactive calls do not wait behind bulk: the broker keeps a worker free for them)
MIN_BULK_CALLS = 1            # a batch holds at least one call, however heavy the function
PROBE_CALLS = 50              # a function never measured runs alone, this many calls, to learn its cost
MIN_LEARN_CALLS = 200         # a mixed batch teaches costs only from this size on
MIN_PROBE_LEARN_CALLS = 8     # a single-function batch is exact enough from here
MIN_COST_US = 0.5
MAX_COST_US = 500_000.0
BATCH_FIXED_MS = 4.0          # fixed cost per batch, and per distinct shape below
SHAPE_FIXED_MS = 1.0
DEFAULT_COST_US = 25.0        # per call, until a function has been measured


class Piece:
    """One submit frame's worth of calls: answered as a unit by (origin, id, seq)."""
    __slots__ = ("conn", "origin", "rid", "seq", "last", "priority", "groups", "calls", "echo", "serial",
                 "t_received", "t_dispatched", "remaining", "frames", "done", "fail", "env_ids", "workers", "primer", "fair",
                 "addins", "source")

    def __init__(self, conn, rid, seq, groups, last, priority, echo=False, serial=False):
        self.conn, self.origin, self.rid, self.seq, self.last = conn, conn.origin, rid, seq, last
        self.fair = conn.origin          # the fair-share key: the broker refines it to origin/client behind a relay
        self.source = {}
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
        self.primer = []        # serial mode: calls evaluated first, their answers dropped
        self.addins = ()        # the add-in files the answering Excel must have open (sorted); () for none


class Item:
    """A slice of one group of one piece, at most CHUNK calls."""
    __slots__ = ("piece", "group", "n", "suspect")

    def __init__(self, piece, group, suspect=False):
        self.piece, self.group, self.n = piece, group, group["n"]
        # suspect: this item crashed Excel (or is an isolated function's call). It runs alone, on the isolation
        # worker only; if it crashes again it is halved until the crashing call stands alone.
        self.suspect = suspect


class Slice:
    """Where an item's calls sit inside a merged group of a batch."""
    __slots__ = ("item", "merged", "offset")

    def __init__(self, item, merged, offset):
        self.item, self.merged, self.offset = item, merged, offset


class Batch:
    __slots__ = ("groups", "slices", "calls", "echo", "serial", "t_created", "pieces", "interactive", "primer", "est_ms",
                 "addins")

    def __init__(self):
        self.addins = ()        # a pinned batch: only a worker with exactly these add-ins open runs it
        self.est_ms = 0.0
        self.groups, self.slices, self.calls = [], [], 0
        self.primer = []
        self.echo = self.serial = False
        self.t_created = time.monotonic()
        self.pieces = set()
        self.interactive = False


def shape_key(group):
    return (group["f"], group["k"], tuple(group["x"]) if "x" in group else None, bool(group.get("spill")))


class _Origin:
    """One origin's queued items, grouped by shape in first-arrival order (a batch then holds few shapes)."""
    __slots__ = ("shapes",)

    def __init__(self):
        self.shapes = {}          # dict keeps insertion order: shape key -> deque of Item

    def empty(self):
        return not self.shapes

    def first(self, allowed=None):
        """The first shape whose head item `allowed` accepts (any, when None), or None."""
        for key, dq in self.shapes.items():
            if allowed is None or allowed(dq[0]):
                return key, dq
        return None


class _Class:
    """Per-class queues with deficit round robin over origins."""

    def __init__(self):
        self.origins = {}
        self.order = deque()             # origins with queued work
        self.deficit = {}
        self.calls = 0

    def add(self, item):
        origin = item.piece.fair
        o = self.origins.get(origin)
        if o is None:
            o = self.origins[origin] = _Origin()
        if o.empty():
            self.order.append(origin)
            self.deficit.setdefault(origin, 0)
        o.shapes.setdefault(shape_key(item.group), deque()).append(item)
        self.calls += item.n

    def push_front(self, item):
        origin = item.piece.fair
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

    def oldest(self):
        """Receive time of the oldest queued item (monotonic), or None."""
        best = None
        for o in self.origins.values():
            for dq in o.shapes.values():
                t = dq[0].piece.t_received
                if best is None or t < best:
                    best = t
        return best

    def items(self):
        for origin in self.order:
            for queue in self.origins[origin].shapes.values():
                yield from queue

    def discard_request(self, rid, origin=None):
        for fair_origin in list(self.order):
            shapes = self.origins[fair_origin].shapes
            for key, queue in list(shapes.items()):
                kept = deque()
                for item in queue:
                    if item.piece.rid == rid and (origin is None or item.piece.origin == origin):
                        self.calls -= item.n
                    else:
                        kept.append(item)
                if kept:
                    shapes[key] = kept
                else:
                    del shapes[key]
            if not shapes:
                self.order.remove(fair_origin)
                self.deficit[fair_origin] = 0

    def take(self, budget_calls, budget_bytes, isolate, costs=None, budget_ms=None, allow_isolated=True):
        """Items by cost-weighted deficit round robin over clients until a budget is spent.

        Returns (items, calls, bytes, estimated_ms).  With budget_ms (bulk) a batch is cut to about that much
        estimated Excel time; a function whose cost was never measured goes alone, PROBE_CALLS at a time, so its
        cost is learnt from a batch that holds nothing else.  An isolated function goes as a one-call batch, and a
        suspect item (one that crashed Excel) alone; both only when allow_isolated (the isolation worker asks)."""
        costs = costs if costs is not None else {}
        taken, calls, size, spent = [], 0, 0, 0.0
        idle_turns = 0
        if allow_isolated:
            def allowed(item):
                return True
        else:
            def allowed(item):
                return not (item.suspect or item.group["f"] in isolate)
        while self.order and calls < budget_calls and size < budget_bytes:
            origin = self.order[0]
            o = self.origins[origin]
            if o.first(allowed) is None:
                # nothing this worker may take from this origin: pass it on, and stop when every origin passes
                self.order.rotate(-1)
                idle_turns += 1
                if idle_turns >= len(self.order):
                    break
                continue
            idle_turns = 0
            self.deficit[origin] += QUANTUM_US
            stop = False
            while not o.empty() and self.deficit[origin] > 0 and calls < budget_calls and size < budget_bytes:
                pick = o.first(allowed)
                if pick is None:
                    break
                key, dq = pick
                item = dq[0]
                f = item.group["f"]
                if item.suspect or f in isolate:
                    # A suspect item, or a function that crashed Excel twice, runs as a batch of its own: the
                    # suspect item whole, an isolated function's call one at a time.
                    if taken:
                        stop = True
                        break
                    if item.suspect:
                        head = dq.popleft()
                        if not dq:
                            del o.shapes[key]
                        self.calls -= head.n
                        if o.empty():
                            self.order.popleft()
                            self.deficit[origin] = 0
                        return [head], head.n, 100 * head.n, costs.get(f, DEFAULT_COST_US) * head.n / 1000
                    if item.n > 1:
                        head, tail = split_item(item, 1)
                        dq[0] = tail
                    else:
                        head = dq.popleft()
                        if not dq:
                            del o.shapes[key]
                    head.suspect = True
                    self.calls -= 1
                    if o.empty():
                        self.order.popleft()
                        self.deficit[origin] = 0
                    return [head], 1, 100, costs.get(f, DEFAULT_COST_US) / 1000
                unit = costs.get(f, DEFAULT_COST_US)
                room = budget_calls - calls
                spill = bool(item.group.get("spill"))
                if taken and spill != bool(taken[0].group.get("spill")):
                    stop = True      # a spill batch holds only spill calls, a scalar batch none
                    break
                if spill:
                    room = min(room, MAX_SPILL_CALLS - calls)
                    if room <= 0:
                        stop = True
                        break
                probe = False
                if budget_ms is not None:
                    if f not in costs:
                        if any(t.group["f"] != f for t in taken):
                            stop = True      # an unmeasured function never shares a batch
                            break
                        room = min(room, PROBE_CALLS - calls)
                        if room <= 0:
                            stop = True
                            break
                        probe = True
                    else:
                        room = min(room, max(MIN_BULK_CALLS, int((budget_ms - spent) * 1000 / unit)))
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
                spent += item.n * unit / 1000
                if budget_ms is not None and (spent >= budget_ms or (probe and calls >= PROBE_CALLS)):
                    stop = True
                size += item.n * max(1, protocol.group_width(item.group)) * APPROX_BYTES_PER_VALUE
                self.calls -= item.n
                self.deficit[origin] -= item.n * unit
                if stop:
                    break
            self.order.popleft()
            if not o.empty():
                self.order.append(origin)
            else:
                self.deficit[origin] = 0
            if stop:
                break
        return taken, calls, size, spent


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
        self.sweep = _Class()
        self.sweep_serial = _Class()
        self.isolate = set()
        self.recent_interactive = 0.0
        self.costs = {}          # function -> estimated microseconds per call
        self.serial = deque()    # serial pieces, oldest first; only the serial worker takes them
        self.pinned = {}         # add-in set -> pieces, oldest first; only a worker with those add-ins open takes them

    def pinned_pending(self, addins):
        return bool(self.pinned.get(addins))

    def next_pinned_batch(self, addins):
        """The oldest piece that needs `addins` open, whole: only a worker with that environment runs it."""
        piece = self.pinned[addins].popleft()
        batch = Batch()
        batch.addins = addins
        batch.echo = piece.echo
        batch.interactive = piece.priority == "interactive"
        for index, group in enumerate(piece.groups):
            batch.groups.append(group)
            batch.slices.append(Slice(Item(piece, group), index, 0))
            batch.calls += group["n"]
        batch.pieces.add(piece)
        piece.remaining = len(piece.groups)
        piece.t_dispatched = time.monotonic()
        batch.est_ms = sum(g["n"] * self.costs.get(g["f"], DEFAULT_COST_US) for g in batch.groups) / 1000
        return batch

    def classes(self):
        return self.interactive, self.bulk, self.sweep, self.sweep_serial

    def queue_for(self, piece):
        if piece.priority == "sweep":
            return self.sweep_serial if piece.serial else self.sweep
        return self.interactive if piece.priority == "interactive" else self.bulk

    def queued_calls(self):
        return (sum(cls.calls for cls in self.classes()) + sum(p.calls for p in self.serial)
                + sum(p.calls for queue in self.pinned.values() for p in queue))

    def waiting(self):
        """Queued groups with their source piece and whether they need isolation."""
        for cls in self.classes():
            for item in cls.items():
                yield item.piece, item.group, item.suspect or item.group["f"] in self.isolate
        for queue in (self.serial, *self.pinned.values()):
            for piece in queue:
                for group in piece.groups:
                    yield piece, group, False

    def discard_request(self, rid, origin=None):
        def matches(piece):
            return piece.rid == rid and (origin is None or piece.origin == origin)
        for cls in self.classes():
            cls.discard_request(rid, origin=origin)
        self.serial = deque(p for p in self.serial if not matches(p))
        for addins, queue in self.pinned.items():
            self.pinned[addins] = deque(p for p in queue if not matches(p))

    def serial_pending(self):
        return bool(self.serial)

    def next_serial_batch(self):
        """The oldest serial piece, whole and in order, with its primer: one worker evaluates it call by call."""
        piece = self.serial.popleft()
        batch = Batch()
        batch.serial = True
        batch.echo = piece.echo
        batch.primer = piece.primer
        for index, group in enumerate(piece.groups):
            batch.groups.append(group)
            batch.slices.append(Slice(Item(piece, group), index, 0))
            batch.calls += group["n"]
        batch.pieces.add(piece)
        piece.remaining = len(piece.groups)
        piece.t_dispatched = time.monotonic()
        return batch

    def add_piece(self, piece):
        if piece.addins:
            self.pinned.setdefault(piece.addins, deque()).append(piece)
            piece.remaining = len(piece.groups)
            return
        if piece.serial and piece.priority != "sweep":
            self.serial.append(piece)
            piece.remaining = len(piece.groups)
            return
        cls = self.queue_for(piece)
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

    def next_batch(self, allow_isolated=True, sweep_queue=None):
        """Assemble the next batch for an idle worker, or None.  allow_isolated=False (every worker but the
        isolation worker) leaves suspect items and isolated functions' calls in the queue."""
        batch = Batch()
        items = []
        if sweep_queue is not None:
            items, calls, _, est = sweep_queue.take(32, BATCH_BYTES, self.isolate, self.costs, BULK_BATCH_MS,
                                              allow_isolated=allow_isolated)
        elif not self.interactive.empty():
            items, calls, _, est = self.interactive.take(BATCH_CALLS, BATCH_BYTES, self.isolate, self.costs,
                                                         allow_isolated=allow_isolated)
            batch.interactive = bool(items)
        if sweep_queue is None and not items and not self.bulk.empty():
            items, calls, _, est = self.bulk.take(BATCH_CALLS, BATCH_BYTES, self.isolate, self.costs, BULK_BATCH_MS,
                                                  allow_isolated=allow_isolated)
        if not items:
            return None
        merged = {}
        for item in items:
            g = item.group
            key = shape_key(g)
            entry = merged.get(key)
            if entry is None:
                m = {"f": g["f"], "k": g["k"], "n": 0, "v": []}
                if "x" in g:
                    m["x"] = g["x"]
                if g.get("spill"):
                    m["spill"] = True
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
        batch.est_ms = est
        now = time.monotonic()
        for piece in batch.pieces:
            if piece.t_dispatched is None:
                piece.t_dispatched = now
        return batch

    def learn(self, batch, actual_ms):
        """Update per-function cost estimates (microseconds per call) from a finished batch.

        A batch of one function gives its cost directly; a mixed batch scales its functions' estimates by how
        far off the prediction was."""
        fns = {g["f"] for g in batch.groups}
        fixed = BATCH_FIXED_MS + SHAPE_FIXED_MS * len(batch.groups)
        if len(fns) == 1:
            if batch.calls < MIN_PROBE_LEARN_CALLS:
                return
            f = next(iter(fns))
            net = max(actual_ms - fixed, 0.2 * actual_ms)
            measured = net * 1000 / batch.calls
            old = self.costs.get(f)
            value = measured if old is None else 0.5 * old + 0.5 * measured
            self.costs[f] = min(MAX_COST_US, max(MIN_COST_US, value))
            return
        if batch.calls < MIN_LEARN_CALLS:
            return           # fixed costs would be charged to the few calls
        actual_ms = max(actual_ms - fixed, 0.2 * actual_ms)
        predicted = sum(g["n"] * self.costs.get(g["f"], DEFAULT_COST_US) for g in batch.groups) / 1000
        if predicted <= 0 or actual_ms <= 0:
            return
        ratio = max(0.25, min(4.0, actual_ms / predicted))
        for g in batch.groups:
            old = self.costs.get(g["f"], DEFAULT_COST_US)
            self.costs[g["f"]] = min(MAX_COST_US, max(MIN_COST_US, old * ratio ** 0.5 if g["f"] in self.costs else old * ratio))

    def oldest_queued(self):
        """Receive time of the oldest queued item, or None when nothing waits."""
        times = [t for cls in self.classes() if (t := cls.oldest()) is not None]
        times += [p.t_received for p in self.serial]
        times += [p.t_received for queue in self.pinned.values() for p in queue]
        return min(times) if times else None

    def requeue_items(self, items):
        """Put items back at the front, in order (after a crash: suspect items to run alone)."""
        for item in reversed(items):
            self.queue_for(item.piece).push_front(item)

    def requeue(self, batch):
        """Put work the worker did not run back in its original queue."""
        if batch.addins or (batch.serial and all(p.priority != "sweep" for p in batch.pieces)):
            queue = self.serial if batch.serial else self.pinned.setdefault(batch.addins, deque())
            for piece in batch.pieces:
                if not piece.done:
                    queue.appendleft(piece)
            return
        self.requeue_items([sl.item for sl in batch.slices if not sl.item.piece.done])
