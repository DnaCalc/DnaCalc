"""touchstone/1 broker: admission, batching, a pool of engine workers, streamed answers.

Runs on Windows under tools/excel-session/on-private-desktop.ps1 (it starts Excel through the C# workers):

    py -3 broker.py start|stop|run|status|stats|waits --state DIR

Engines are worker processes (engine.py).  Clients are local TCP connections (token file under the state
folder), the Linux relays' ssh streams (peers.py) and the v1 file queue (v1 clients keep working).
No DnaCalc imports: admission is a plug-in (--admission module:factory).
"""
import argparse
import asyncio
import collections
import hashlib
import importlib
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))

import batcher  # noqa: E402
import engine  # noqa: E402
import logs  # noqa: E402
import protocol  # noqa: E402
import v1  # noqa: E402

DEFAULT_STATE = Path(r"C:\Work\DnaCalc\codex-runs\excel-service-v2")
ANSWER_CACHE_SECONDS = 15 * 60
ANSWER_CACHE_BYTES = 256 * 1024 * 1024
FRAME_CALLS = 2000
BASE_DEADLINE_MS = 5000
DEADLINE_MS_PER_CALL = 0.1
# Functions whose answers depend on the call before them (measured, 2026-11-08: WEEKNUM with a return type
# outside 1, 2, 11-17, 21): they are always evaluated call by call in request order on the serial worker.
STATE_SENSITIVE = ("WEEKNUM",)
# A worker's Excel is replaced, one worker at a time, after this much work (leftovers and leaks stay bounded).
RECYCLE_CALLS = 5_000_000
RECYCLE_SECONDS = 4 * 3600
RECYCLE_BATCHES = 10_000
WORKER_MT = (4, 4, 0)             # threads per worker; 0 = multithreaded calculation off (the serial worker)


class OpenAdmission:
    """Structure only: known tokens, matching value counts.  DnaCalc supplies the real policy."""

    def check(self, group):
        protocol.group_width(group)
        if len(group["v"]) != group["n"] * protocol.group_width(group):
            raise ValueError("value count does not match the group's shape")

    def v1(self, raw, rid):
        return v1.parse_request(raw, rid)


def load_admission(spec):
    if not spec:
        return OpenAdmission()
    module, _, attr = spec.partition(":")
    mod = importlib.import_module(module)
    return getattr(mod, attr or "admission")(HERE.parents[2])


class Conn:
    """A client connection: framed reader/writer, prioritised outgoing queue."""

    def __init__(self, origin, reader, writer, compress=False):
        self.origin, self.reader, self.writer, self.compress = origin, reader, writer, compress
        self.high, self.low = asyncio.Queue(), asyncio.Queue()
        self.wake = asyncio.Event()
        self.sent_envs = set()
        self.closed = False
        self.task = None

    def send(self, kind, obj, high=False):
        if self.closed:
            return
        (self.high if high else self.low).put_nowait(protocol.pack(kind, obj, self.compress))
        self.wake.set()

    async def writer_loop(self):
        try:
            while not self.closed:
                await self.wake.wait()
                self.wake.clear()
                while not (self.high.empty() and self.low.empty()):
                    queue = self.high if not self.high.empty() else self.low
                    self.writer.write(queue.get_nowait())
                    await self.writer.drain()
        except (ConnectionError, OSError, asyncio.CancelledError):
            pass
        finally:
            self.closed = True

    def close(self):
        self.closed = True
        self.wake.set()
        try:
            self.writer.close()
        except Exception:
            pass


class V1Conn:
    """Stands in for a connection when a v1 request file is answered: collects frames into a reply."""

    def __init__(self, reply, finish):
        self.origin = "v1-local"
        self.reply, self.finish = reply, finish
        self.closed = False
        self.sent_envs = set()
        self.compress = False

    def send(self, kind, obj, high=False):
        if kind == protocol.ANSWER:
            self.reply.add(obj)
        elif kind == protocol.ERROR:
            self.reply.fail(obj.get("code", "error"), obj.get("detail"))
            self.finish()
        elif kind == protocol.DONE:
            self.finish()

    def close(self):
        self.closed = True


class Request:
    __slots__ = ("pieces", "last_seq", "priority", "echo", "serial", "t_first", "calls", "done_sent", "cache_bytes", "envs", "touched", "client", "primer")

    def __init__(self):
        self.pieces, self.last_seq = {}, None
        self.priority, self.echo, self.serial = "bulk", False, False
        self.t_first = time.monotonic()
        self.calls = 0
        self.done_sent = False
        self.cache_bytes = 0
        self.envs = set()
        self.touched = time.monotonic()
        self.client = None
        self.primer = []


class Slot:
    """One engine worker in the pool."""

    def __init__(self, name, mt, state, root, log_dir, exe):
        self.name, self.mt = name, mt
        self.proc = engine.WorkerProcess(root, name, mt=mt, log_dir=log_dir, exe=exe)
        self.state = "new"          # new, idle, busy, down
        self.serial = mt == 0       # the serial worker: multithreaded calculation off, takes serial requests
        self.hello = None
        self.batches = 0
        self.calls = 0
        self.crashes = 0
        self.restarts = 0
        self.since = {"time": time.time(), "calls": 0, "batches": 0}   # since this Excel started


class Broker:
    def __init__(self, options):
        self.options = options
        self.state_dir = options.state.resolve()
        self.loop = None
        self.batcher = batcher.Batcher()
        self.admission = load_admission(options.admission)
        self.requests = {}
        self.slots = []
        self.executor = ThreadPoolExecutor(max_workers=16, thread_name_prefix="engine")
        self.batch_no = 0
        self.stopping = False
        self.cache_bytes = 0
        self.counters = {"calls": 0, "batches": 0, "requests": 0, "refused": 0, "errors": 0}
        self.crash_counts = {}
        registry = getattr(options, "registry", None)
        self.registry = set(STATE_SENSITIVE if registry is None else [] if registry == ["none"] else registry)
        self.leak_every = getattr(options, "leak_every", 100)
        self.recent = collections.deque()       # (group, ty, v) of recent scalar answers, for the fresh-Excel sample
        self.recent_calls = 0
        self.leak_counter = 0
        self.pending_checks = collections.deque()
        self.leak_checks = {"checked": 0, "differences": 0, "skipped": 0}
        self.last_fresh = time.time()
        self.fresh_running = False
        self.build_changed = False
        self.started = time.time()
        self.machine = socket.gethostname()
        self.cpu = None
        self.conns = []

    # ---- setup ------------------------------------------------------------------------
    def prepare(self):
        for name in ("log", "excel"):
            (self.state_dir / name).mkdir(parents=True, exist_ok=True)
        self.batch_log = logs.BatchLog(self.state_dir / "log")
        self.waits = logs.WaitsLog(self.state_dir / "log" / "waits.jsonl")
        self.envs = logs.EnvLog(self.state_dir / "envs.jsonl")

    def cpu_name(self):
        try:
            out = subprocess.run(["pwsh", "-NoProfile", "-NonInteractive", "-Command",
                                  "(Get-CimInstance Win32_Processor | Select-Object -First 1).Name"],
                                 capture_output=True, text=True, timeout=30, creationflags=subprocess.CREATE_NO_WINDOW)
            return out.stdout.strip()
        except Exception:
            return None

    def stage_worker(self):
        """Run the workers from a copy of the build, so a rebuild never meets a locked executable."""
        import shutil
        source = engine.WORKER_EXE.parent
        dll = source / "excel-worker.dll"
        tag = hashlib.sha1(f"{dll.stat().st_mtime_ns}:{dll.stat().st_size}".encode()).hexdigest()[:10]
        target = self.state_dir / "bin" / tag
        if not (target / "excel-worker.exe").exists():
            shutil.copytree(source, target, dirs_exist_ok=True)
        return target / "excel-worker.exe"

    async def start_pool(self):
        self.worker_exe = getattr(self.options, "worker_command", None) or self.stage_worker()
        self.cpu = await self.loop.run_in_executor(self.executor, self.cpu_name)
        count = self.options.workers
        for i in range(count):
            mt = WORKER_MT[i] if i < len(WORKER_MT) else 4
            self.slots.append(Slot(f"w{i + 1}", mt, self.state_dir, self.state_dir / "excel", self.state_dir / "excel" / "worker-logs", self.worker_exe))
        await asyncio.gather(*(self.start_slot(s) for s in self.slots))

    async def start_slot(self, slot):
        try:
            hello = await self.loop.run_in_executor(self.executor, slot.proc.start)
        except Exception as error:
            slot.state = "down"
            self.waits.add("worker-down", worker=slot.name, seconds=0.0, detail=str(error)[:300])
            self.log_event("worker-down", worker=slot.name, detail=str(error)[:300])
            return False
        self.adopt_hello(slot, hello)
        return True

    def adopt_hello(self, slot, hello):
        env = dict(hello["env"])
        if self.cpu:
            env["cpu"] = self.cpu
        # The worker computed env_id without the broker's CPU name; keep its id so answers and worker agree.
        self.envs.add(hello["env_id"], env, {"worker": slot.name})
        slot.hello = hello
        slot.since = {"time": time.time(), "calls": 0, "batches": 0}
        slot.state = "idle"
        self.log_event("worker-up", worker=slot.name, pid=hello["pid"], env=hello["env_id"], restart=hello.get("restart", False))

    def log_event(self, event, **data):
        self.batch_log.write({"kind": "event", "event": event, **data})

    # ---- connections ------------------------------------------------------------------
    async def serve(self, conn):
        self.conns.append(conn)
        conn.task = asyncio.create_task(conn.writer_loop())
        reader = protocol.FrameReader()
        try:
            while not conn.closed:
                data = await conn.reader.read(1 << 20)
                if not data:
                    break
                for kind, body in reader.feed(data):
                    self.on_frame(conn, kind, body)
        except (ConnectionError, OSError, ValueError):
            pass
        finally:
            conn.close()
            if conn in self.conns:
                self.conns.remove(conn)

    def on_frame(self, conn, kind, body):
        if kind == protocol.HELLO:
            if getattr(conn, "token", None) is not None and body.get("token") != conn.token:
                conn.send(protocol.ERROR, {"code": "auth"}, True)
                conn.close()
                return
            conn.compress = bool(body.get("compress", conn.compress))
            if getattr(conn, "token", None) is not None and body.get("name"):
                # A local client is known by its name, so a reconnect with the same name resumes its requests.
                conn.origin = "tcp:" + str(body["name"])[:40]
            conn.send(protocol.HELLO, {"v": protocol.VERSION, "server": self.machine,
                                       "envs": {k: v["env"] for k, v in self.envs.known.items()}}, True)
        elif kind == protocol.SUBMIT:
            self.on_submit(conn, body)
        elif kind == protocol.PING:
            conn.send(protocol.PONG, body, True)
        elif kind == protocol.WAIT:
            self.waits.add(body.get("kind", "client-timeout") if body.get("kind") in logs.WAIT_KINDS else "client-timeout",
                           origin=conn.origin, reason=str(body.get("reason"))[:200], seconds=float(body.get("seconds", 0)),
                           lane=str(body.get("lane"))[:80])
        elif kind == protocol.CANCEL:
            pass

    def on_submit(self, conn, body):
        rid, seq = body["id"], body["seq"]
        key = (conn.origin, rid)
        req = self.requests.get(key)
        if req is None:
            req = self.requests[key] = Request()
            req.client = body.get("c")
            self.counters["requests"] += 1
            opts = body.get("opts") or {}
            req.echo = bool(opts.get("echo"))
            req.serial = opts.get("order") == "serial"
            req.primer = opts.get("primer") or []
            total = opts.get("calls", 0)
            req.priority = opts.get("priority") or ("interactive" if 0 < total <= batcher.INTERACTIVE_CALLS else "bulk")
        req.touched = time.monotonic()
        if body.get("last"):
            req.last_seq = seq
        existing = req.pieces.get(seq)
        if existing is not None:
            # The client resent an unanswered piece (reconnect): rebind, replay what exists, never re-evaluate.
            existing.conn = conn
            req.client = body.get("c", req.client)
            for frame in existing.frames:
                self.send_answer(conn, self.tag(dict(frame), req.client), existing)
            if req.done_sent and body.get("last"):
                conn.send(protocol.DONE, self.tag({"id": rid, "calls": req.calls, "envs": sorted(req.envs), "seconds": 0.0,
                                                   "replayed": True}, req.client), req.priority == "interactive")
            else:
                self.maybe_done(key, req)
            return
        groups = body["groups"]
        try:
            for group in [*groups, *(req.primer if seq == 0 else [])]:
                self.admission.check(group)
        except ValueError as error:
            self.counters["refused"] += 1
            self.waits.add("refused-kind", origin=conn.origin, function=groups[0]["f"] if groups else None,
                           seconds=0.0, detail=str(error)[:200])
            piece = batcher.Piece(conn, rid, seq, groups, bool(body.get("last")), req.priority)
            piece.done, piece.fail = True, ("refused", str(error))
            req.pieces[seq] = piece
            conn.send(protocol.ERROR, self.tag({"id": rid, "seq": seq, "code": "refused", "detail": str(error)}, req.client), True)
            self.maybe_done(key, req)
            return
        serial = req.serial or any(g["f"] in self.registry for g in groups)
        piece = batcher.Piece(conn, rid, seq, groups, bool(body.get("last")), req.priority, req.echo, serial)
        if serial and seq == 0:
            piece.primer = [dict(g, p=True) for g in req.primer]
        req.pieces[seq] = piece
        req.calls += piece.calls
        self.batcher.add_piece(piece)
        self.kick()

    # ---- dispatch ---------------------------------------------------------------------
    def kick(self):
        # Serial work goes to the serial worker, in order; while any waits, ordinary batches keep off that worker.
        if self.batcher.serial_pending():
            serial_slots = [s for s in self.slots if s.serial and s.state != "down"]
            if not serial_slots:
                while self.batcher.serial_pending():
                    batch = self.batcher.next_serial_batch()
                    self.fail_batch(batch, "worker-down", "no serial worker is up")
            else:
                slot = next((s for s in serial_slots if s.state == "idle"), None)
                if slot is not None:
                    batch = self.batcher.next_serial_batch()
                    slot.state = "busy"
                    asyncio.create_task(self.run_batch(slot, batch))
        keep_free = self.batcher.serial_pending()
        while self.batcher.has_work():
            slot = next((s for s in self.slots if s.state == "idle" and not (keep_free and s.serial)), None)
            if slot is None:
                if self.slots and all(s.state == "down" for s in self.slots):
                    self.fail_all("no worker is up")
                return
            batch = self.batcher.next_batch()
            if batch is None:
                return
            slot.state = "busy"
            asyncio.create_task(self.run_batch(slot, batch))
        self.run_pending_checks()
        self.maybe_recycle()

    def maybe_recycle(self):
        """Staggered: at most one worker is being replaced, and never the last one that is up."""
        if any(s.state == "new" for s in self.slots):
            return
        limit_calls = getattr(self.options, "recycle_calls", RECYCLE_CALLS)
        limit_batches = getattr(self.options, "recycle_batches", RECYCLE_BATCHES)
        limit_seconds = getattr(self.options, "recycle_seconds", RECYCLE_SECONDS)
        up = [s for s in self.slots if s.state in ("idle", "busy")]
        for slot in up:
            old = (slot.since["calls"] >= limit_calls or slot.since["batches"] >= limit_batches
                   or time.time() - slot.since["time"] >= limit_seconds)
            if slot.state == "idle" and old and len(up) > 1:
                slot.state = "new"
                asyncio.create_task(self.recycle_slot(slot))
                return

    async def recycle_slot(self, slot):
        reason = dict(slot.since)
        try:
            hello = await self.loop.run_in_executor(self.executor, slot.proc.recycle)
            self.adopt_hello(slot, hello)
            self.log_event("recycled", worker=slot.name, after=reason)
        except Exception as error:
            self.log_event("recycle-failed", worker=slot.name, detail=str(error)[:200])
            await self.restart_slot(slot)
        self.kick()

    @staticmethod
    def tag(obj, client):
        if client is not None:
            obj["c"] = client
        return obj

    def fail_all(self, detail):
        while self.batcher.has_work():
            batch = self.batcher.next_batch()
            if batch is None:
                break
            self.fail_batch(batch, "worker-down", detail)

    async def run_batch(self, slot, batch):
        self.batch_no += 1
        number = self.batch_no
        calls = batch.calls
        deadline_ms = BASE_DEADLINE_MS + DEADLINE_MS_PER_CALL * calls
        message = {"id": number, "groups": [*batch.primer, *batch.groups], "deadline_ms": deadline_ms, "echo": batch.echo,
                   "serial": batch.serial}
        t_dispatch = time.monotonic()
        result, failure = None, None
        try:
            result = await self.loop.run_in_executor(self.executor, slot.proc.run, message, deadline_ms / 1000 + 5)
        except engine.WorkerTimeout as error:
            failure = ("excel-timeout", str(error))
        except Exception as error:
            failure = ("worker-lost", str(error)[:300])
        t_back = time.monotonic()
        restart = False
        if failure is None and result.get("error"):
            error = result["error"]
            detail = result.get("detail")
            if error == "fingerprint-drift-before":
                # nothing ran: give the work back to the queue
                self.batcher.requeue(batch)
                self.log_event("drift-before", worker=slot.name, detail=detail)
                await self.recycle_after_worker_event(slot)
                self.kick()
                return
            failure = (error, detail)
            restart = error in ("excel-crashed", "fingerprint-drift-after")
            if error == "excel-crashed":
                slot.crashes += 1
                for item_f in {g["f"] for g in batch.groups}:
                    self.crash_counts[item_f] = self.crash_counts.get(item_f, 0) + 1
                    if self.crash_counts[item_f] >= 2:
                        self.batcher.isolate.add(item_f)
        if failure is not None:
            self.fail_batch(batch, failure[0], failure[1])
            self.log_batch(slot, number, batch, None, t_dispatch, t_back, error=failure[0])
            self.waits.add("excel-start" if failure[0] in ("worker-lost", "excel-timeout") else "excel-calc",
                           worker=slot.name, seconds=round(t_back - t_dispatch, 3), detail=str(failure[0])[:100])
            if failure[0] in ("excel-timeout", "worker-lost"):
                await self.restart_slot(slot)
            elif restart:
                await self.finish_restart(slot)
            else:
                slot.state = "idle"
            self.kick()
            return
        env_id = result["env_id"]
        self.respond(batch, result, slot)
        self.remember(batch, result)
        self.leak_check_maybe(slot, batch, result)
        t_sent = time.monotonic()
        t = result.get("timing", {})
        variable = sum(t.get(k) or 0 for k in ("write_ms", "calc_ms", "read_ms", "convert_ms"))
        self.batcher.learn(batch, variable or (t_back - t_dispatch) * 1000)
        slot.batches += 1
        slot.calls += calls
        slot.since["batches"] += 1
        slot.since["calls"] += calls
        self.counters["batches"] += 1
        self.counters["calls"] += calls
        self.log_batch(slot, number, batch, result, t_dispatch, t_back, sent=t_sent)
        slot.state = "idle"
        self.kick()

    async def recycle_after_worker_event(self, slot):
        # The worker recycled its Excel itself (drift); it announces a new hello.
        try:
            hello = await self.loop.run_in_executor(self.executor, slot.proc.await_restart)
            self.adopt_hello(slot, hello)
        except Exception as error:
            slot.state = "down"
            self.waits.add("worker-down", worker=slot.name, seconds=0.0, detail=str(error)[:300])

    async def finish_restart(self, slot):
        await self.recycle_after_worker_event(slot)

    async def restart_slot(self, slot):
        """After a missed deadline or a lost worker process: start it once more; a failed start leaves it down."""
        slot.restarts += 1
        slot.state = "new"
        await self.loop.run_in_executor(self.executor, slot.proc.kill)
        if slot.restarts > 6:
            slot.state = "down"
            self.waits.add("worker-down", worker=slot.name, seconds=0.0, detail="too many restarts")
            return
        slot.proc = engine.WorkerProcess(slot.proc.root, slot.name, mt=slot.mt, log_dir=slot.proc.log_dir, exe=slot.proc.exe)
        await self.start_slot(slot)

    # ---- reversed-order and fresh-Excel checks ---------------------------------------------
    def remember(self, batch, result):
        """Keep recent scalar answers (calls and their answers) for the fresh-Excel sample."""
        if batch.serial or any(g.get("spill") for g in batch.groups) or result.get("order"):
            return
        for g, a in zip(batch.groups, result["groups"]):
            if "ity" in a:
                a = {"ty": a["ty"], "v": a["v"]}
            self.recent.append(({k: v for k, v in g.items() if k in ("f", "k", "n", "v", "x")}, a["ty"], a["v"]))
            self.recent_calls += g["n"]
        while self.recent_calls > 40000 and self.recent:
            old = self.recent.popleft()
            self.recent_calls -= old[0]["n"]

    @staticmethod
    def reversed_groups(groups):
        out = []
        for g in reversed(groups):
            width = protocol.group_width(g)
            n = g["n"]
            v = g["v"]
            reversed_v = [v[(n - 1 - i) * width + j] for i in range(n) for j in range(width)]
            r = {"f": g["f"], "k": g["k"], "n": n, "v": reversed_v}
            if "x" in g:
                r["x"] = g["x"]
            out.append(r)
        return out

    def leak_check_maybe(self, slot, batch, result, force=False):
        """One batch in `leak_every`: the same calls in reversed order on another worker, answers compared.

        A difference is a candidate for the state-sensitive registry (leakage.jsonl); no answer is changed."""
        if batch.serial or batch.interactive or any(g.get("spill") for g in batch.groups):
            return
        self.leak_counter += 1
        if not force and (not self.leak_every or self.leak_counter % self.leak_every):
            return
        if len(self.pending_checks) >= 500:
            self.leak_checks["skipped"] += 1
            return
        self.pending_checks.append((slot, batch, result))

    def run_pending_checks(self):
        """Reversed-order checks run on an idle worker other than the first one, when no real work is waiting."""
        while self.pending_checks and not self.batcher.has_work() and not self.batcher.serial_pending():
            first, batch, result = self.pending_checks[0]
            other = next((s for s in self.slots if s.state == "idle" and s is not first and not s.serial), None) or \
                next((s for s in self.slots if s.state == "idle" and s is not first), None)
            if other is None:
                return
            self.pending_checks.popleft()
            other.state = "busy"
            asyncio.create_task(self.leak_check(other, first.name, batch, result))

    async def leak_check(self, other, first_worker, batch, result):
        message = {"id": -self.leak_counter, "groups": self.reversed_groups(batch.groups), "deadline_ms": 30000,
                   "echo": False, "serial": False}
        try:
            second = await self.loop.run_in_executor(self.executor, other.proc.run, message, 40)
        except Exception as error:
            self.log_event("leak-check-failed", worker=other.name, detail=str(error)[:200])
            await self.restart_slot(other)
            self.kick()
            return
        other.state = "idle"
        if second.get("error"):
            self.log_event("leak-check-error", worker=other.name, detail=second.get("error"))
            if second["error"] in ("excel-crashed", "fingerprint-drift-after", "fingerprint-drift-before"):
                await self.recycle_after_worker_event(other)
            self.kick()
            return
        differences = self.compare_reversed(batch.groups, result["groups"], second["groups"], [first_worker, other.name],
                                            [result["env_id"], second["env_id"]], "reversed")
        self.leak_checks["checked"] += 1
        self.leak_checks["differences"] += differences
        self.kick()

    def compare_reversed(self, groups, first, second, workers, envs, kind):
        count = 0
        # second answers the reversed groups in the reversed order: group index k of `second` is groups[-1-k]
        n_groups = len(groups)
        for k, b in enumerate(second):
            gi = n_groups - 1 - k
            g, a = groups[gi], first[gi]
            n, width = g["n"], protocol.group_width(g)
            for i in range(n):
                j = n - 1 - i
                x = (a["ty"][i], a["v"][i])
                y = (b["ty"][j], b["v"][j])
                if x != y:
                    count += 1
                    self.leak_log({"kind": kind, "function": g["f"], "shape": g["k"], "x": g.get("x"),
                                   "args": g["v"][i * width:(i + 1) * width], "first": list(x), "second": list(y),
                                   "workers": workers, "envs": envs})
        return count

    def leak_log(self, record):
        record = {"utc": logs.utc().isoformat(timespec="milliseconds"), **record}
        with (self.state_dir / "log" / "leakage.jsonl").open("a", encoding="ascii") as stream:
            stream.write(logs.compact(record) + "\n")

    async def fresh_sample(self, calls=10000, reason="daily"):
        """Run a sample of recent calls in a brand-new Excel and compare with the answers that were given."""
        if not self.recent:
            return {"calls": 0, "reason": reason}
        picked, total = [], 0
        for group, ty, v in reversed(self.recent):
            if total >= calls:
                break
            picked.append((group, ty, v))
            total += group["n"]
        proc = engine.WorkerProcess(self.state_dir / "excel", "fresh", mt=4, log_dir=self.state_dir / "excel" / "worker-logs",
                                    exe=self.worker_exe)
        try:
            hello = await self.loop.run_in_executor(self.executor, proc.start)
            message = {"id": 0, "groups": [g for g, _, _ in picked], "deadline_ms": 60000, "echo": False, "serial": False}
            fresh = await self.loop.run_in_executor(self.executor, proc.run, message, 90)
        except Exception as error:
            self.log_event("fresh-sample-failed", detail=str(error)[:200])
            return {"error": str(error)[:200]}
        finally:
            await self.loop.run_in_executor(self.executor, proc.stop)
        if fresh.get("error"):
            return {"error": fresh["error"]}
        different = 0
        for (g, ty, v), b in zip(picked, fresh["groups"]):
            width = protocol.group_width(g)
            for i in range(g["n"]):
                x, y = (ty[i], v[i]), (b["ty"][i], b["v"][i])
                if x != y:
                    different += 1
                    self.leak_log({"kind": "fresh", "function": g["f"], "shape": g["k"], "x": g.get("x"),
                                   "args": g["v"][i * width:(i + 1) * width], "first": list(x), "second": list(y),
                                   "workers": ["pool", "fresh"], "envs": [hello["env_id"]]})
        summary = {"calls": total, "different": different, "reason": reason, "env": hello["env_id"],
                   "build": hello["env"].get("build")}
        self.log_event("fresh-sample", **summary)
        self.last_fresh = time.time()
        return summary

    # ---- answers ----------------------------------------------------------------------
    def send_answer(self, conn, frame, piece):
        env_id = frame.get("env")
        if env_id and env_id not in conn.sent_envs and env_id in self.envs.known:
            conn.sent_envs.add(env_id)
            conn.send(protocol.ENV, {"env_id": env_id, "env": self.envs.known[env_id]["env"]}, True)
        conn.send(protocol.ANSWER, frame, piece.priority == "interactive")

    def respond(self, batch, result, slot):
        env_id = result["env_id"]
        groups = result["groups"]
        for sl in batch.slices:
            item = sl.item
            piece = item.piece
            if piece.fail:
                continue
            ag = groups[sl.merged]
            g = item.group
            width = max(1, protocol.group_width(g)) if "ity" in ag else 0
            frame_groups = []
            for s in range(0, item.n, FRAME_CALLS):
                e = min(item.n, s + FRAME_CALLS)
                fg = {"ty": ag["ty"][sl.offset + s:sl.offset + e], "v": ag["v"][sl.offset + s:sl.offset + e]}
                if "o" in g:
                    fg["o"] = g["o"] + s
                else:
                    fg["i"] = g["i"][s:e]
                if width:
                    fg["ity"] = ag["ity"][(sl.offset + s) * width:(sl.offset + e) * width]
                    fg["iv"] = ag["iv"][(sl.offset + s) * width:(sl.offset + e) * width]
                frame_groups.append(fg)
            req = self.requests.get((piece.origin, piece.rid))
            frame = self.tag({"id": piece.rid, "seq": piece.seq, "env": env_id, "w": slot.name, "b": result["batch"],
                              "g": frame_groups}, req.client if req else None)
            if batch.serial:
                frame["order"] = "serial"
            piece.frames.append(frame)
            piece.env_ids.add(env_id)
            piece.workers.add(slot.name)
            self.send_answer(piece.conn, frame, piece)
            piece.remaining -= 1
            if piece.remaining <= 0:
                self.finish_piece(piece)

    def fail_batch(self, batch, code, detail):
        seen = set()
        for sl in batch.slices:
            piece = sl.item.piece
            if piece in seen:
                continue
            seen.add(piece)
            piece.fail = (code, detail)
            piece.remaining = 0
            req = self.requests.get((piece.origin, piece.rid))
            piece.conn.send(protocol.ERROR, self.tag({"id": piece.rid, "seq": piece.seq, "code": code, "detail": str(detail)[:400]},
                                                     req.client if req else None), piece.priority == "interactive")
            self.finish_piece(piece)
            self.counters["errors"] += 1

    def finish_piece(self, piece):
        if piece.done:
            return
        piece.done = True
        key = (piece.origin, piece.rid)
        req = self.requests.get(key)
        if req is None:
            return
        req.envs |= piece.env_ids
        for frame in piece.frames:
            req.cache_bytes += 40 + sum(len(g["ty"]) * 24 for g in frame["g"])
        self.cache_bytes += sum(40 + sum(len(g["ty"]) * 24 for g in f["g"]) for f in piece.frames)
        self.maybe_done(key, req)

    def maybe_done(self, key, req):
        if req.done_sent or req.last_seq is None:
            return
        if len(req.pieces) != req.last_seq + 1 or not all(p.done for p in req.pieces.values()):
            return
        req.done_sent = True
        elapsed = time.monotonic() - req.t_first
        piece = req.pieces[req.last_seq]
        piece.conn.send(protocol.DONE, self.tag({"id": key[1], "calls": req.calls, "envs": sorted(req.envs),
                                                 "seconds": round(elapsed, 4)}, req.client), req.priority == "interactive")
        budget = 1.0 + 10e-6 * req.calls
        queued = max(((p.t_dispatched or p.t_received) - p.t_received for p in req.pieces.values()), default=0)
        if queued > 0.25:
            self.waits.add("queue", origin=key[0], id=key[1][:8], calls=req.calls, seconds=round(queued, 3))
        if elapsed > budget:
            self.waits.add("queue" if queued > elapsed / 2 else "excel-calc", origin=key[0], id=key[1][:8], calls=req.calls,
                           seconds=round(elapsed, 3), budget=round(budget, 3))
        req.touched = time.monotonic()
        self.evict()

    def evict(self):
        now = time.monotonic()
        if self.cache_bytes < ANSWER_CACHE_BYTES and len(self.requests) < 20000:
            old = [k for k, r in self.requests.items() if r.done_sent and now - r.touched > ANSWER_CACHE_SECONDS]
        else:
            old = [k for k, r in sorted(self.requests.items(), key=lambda kv: kv[1].touched) if r.done_sent][:max(1, len(self.requests) // 4)]
        for k in old:
            r = self.requests.pop(k)
            self.cache_bytes -= r.cache_bytes

    def log_batch(self, slot, number, batch, result, t_dispatch, t_back, sent=None, error=None):
        fns = {}
        for g in batch.groups:
            fns[g["f"]] = fns.get(g["f"], 0) + g["n"]
        cells = sum(g["n"] * (protocol.group_width(g) + 1) for g in batch.groups)
        queued = max(((p.t_dispatched or p.t_dispatched) - p.t_received for p in batch.pieces if p.t_dispatched), default=0)
        t = (result or {}).get("timing", {})
        record = {"kind": "batch", "batch": number, "worker": slot.name, "env": (slot.hello or {}).get("env_id"),
                  "calls": batch.calls, "cells": cells, "fns": fns, "pieces": len(batch.pieces),
                  "reqs": sorted({p.rid[:8] for p in batch.pieces})[:8],
                  "ms": {"queued": round(queued * 1000, 2), "write": t.get("write_ms"), "calc": t.get("calc_ms"),
                         "read": t.get("read_ms"), "convert": t.get("convert_ms"), "fp": t.get("fingerprint_ms"), "gc": t.get("gc_pause_ms"), "total": t.get("total_ms") or round((t_back - t_dispatch) * 1000, 2),
                         "round": round((t_back - t_dispatch) * 1000, 2),
                         "sent": round(((sent or t_back) - t_back) * 1000, 2)}}
        if error:
            record["error"] = error
        self.batch_log.write(record)

    # ---- v1 local queue ---------------------------------------------------------------
    async def v1_scan(self, queue_dir):
        queue_dir = Path(queue_dir)
        (queue_dir / "in").mkdir(parents=True, exist_ok=True)
        (queue_dir / "out").mkdir(parents=True, exist_ok=True)
        seen = {}
        while not self.stopping:
            try:
                for path in (queue_dir / "in").glob("*.json"):
                    if len(path.stem) != 32 or path.is_symlink():
                        continue
                    stamp = path.stat().st_mtime_ns
                    if seen.get(path.name) == stamp:
                        continue
                    seen[path.name] = stamp
                    asyncio.create_task(self.v1_handle(queue_dir, path))
            except OSError:
                pass
            await asyncio.sleep(0.02)

    async def v1_handle(self, queue_dir, path):
        rid = path.stem
        try:
            raw = path.read_bytes()
        except OSError:
            return
        digest = hashlib.sha256(raw).hexdigest()
        out = queue_dir / "out" / path.name
        if out.exists():
            return
        done = asyncio.Event()
        try:
            function, rows, index = self.admission.v1(raw, rid)
            frames = v1.submit_frames(rid, function, rows, index, echo=True)
            reply = v1.Reply(rid, function, rows, index, digest, self.envs.known, self.machine)
        except Exception as error:
            reply = v1.Reply(rid, None, [], None, digest, self.envs.known, self.machine)
            reply.fail("refused", str(error))
            frames = []
            self.counters["refused"] += 1
            self.waits.add("refused-kind", origin="v1-local", detail=str(error)[:200], seconds=0.0)
            done.set()
        conn = V1Conn(reply, done.set)
        for frame in frames:
            self.on_submit(conn, frame)
        try:
            await asyncio.wait_for(done.wait(), 900)
        except asyncio.TimeoutError:
            reply.fail("timeout")
        answer = reply.build()
        temporary = out.with_suffix(".tmp")
        with temporary.open("w", encoding="utf-8") as stream:
            json.dump(answer, stream, allow_nan=False, ensure_ascii=True)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, out)
        path.unlink(missing_ok=True)

    # ---- local TCP endpoint -----------------------------------------------------------
    async def tcp_endpoint(self):
        token = secrets.token_hex(16)

        async def accept(reader, writer):
            conn = Conn("tcp:%d" % writer.get_extra_info("peername")[1], reader, writer)
            conn.token = token
            await self.serve(conn)

        server = await asyncio.start_server(accept, "127.0.0.1", 0)
        port = server.sockets[0].getsockname()[1]
        tmp = self.state_dir / "endpoint.json.tmp"
        tmp.write_text(json.dumps({"host": "127.0.0.1", "port": port, "token": token, "pid": os.getpid()}), encoding="ascii")
        os.replace(tmp, self.state_dir / "endpoint.json")
        return server

    # ---- main -------------------------------------------------------------------------
    def note_build(self):
        """An Excel build that differs from the last run's schedules a fresh-Excel sample once traffic exists."""
        builds = {(s.hello or {}).get("env", {}).get("build") for s in self.slots if s.hello}
        path = self.state_dir / "last-build.txt"
        previous = path.read_text(encoding="ascii").strip() if path.exists() else None
        current = ",".join(sorted(b for b in builds if b))
        self.build_changed = bool(previous) and previous != current
        path.write_text(current, encoding="ascii")

    def schedule_fresh(self):
        trigger = self.state_dir / "leak-now"
        reason = None
        if trigger.exists():
            reason = "requested"
        elif self.recent and time.time() - self.last_fresh > 24 * 3600:
            reason = "daily"
        elif self.build_changed and self.recent and time.time() - self.started > 120:
            reason = "build-change"
            self.build_changed = False
        if reason and not self.fresh_running and self.recent:
            trigger.unlink(missing_ok=True)
            self.fresh_running = True

            async def run():
                try:
                    summary = await self.fresh_sample(reason=reason)
                    (self.state_dir / "fresh-sample.json").write_text(json.dumps(summary), encoding="ascii")
                finally:
                    self.fresh_running = False
            asyncio.create_task(run())

    def write_status(self):
        status = {"pid": os.getpid(), "running": True, "utc": logs.utc().isoformat(timespec="seconds"),
                  "queued_calls": self.batcher.queued_calls(), "counters": self.counters,
                  "workers": [{"name": s.name, "state": s.state, "batches": s.batches, "calls": s.calls,
                               "pid": (s.hello or {}).get("pid"), "env": (s.hello or {}).get("env_id"),
                               "mt": s.mt, "crashes": s.crashes} for s in self.slots],
                  "isolated_functions": sorted(self.batcher.isolate), "answer_cache_bytes": self.cache_bytes,
                  "leak_checks": self.leak_checks, "pending_leak_checks": len(self.pending_checks), "serial_registry": sorted(self.registry)}
        tmp = self.state_dir / "status.json.tmp"
        tmp.write_text(json.dumps(status), encoding="ascii")
        try:
            os.replace(tmp, self.state_dir / "status.json")
        except OSError:
            pass    # a reader holds the file open (Windows); the next tick publishes it

    async def run(self):
        import msvcrt
        self.loop = asyncio.get_running_loop()
        self.prepare()
        with (self.state_dir / "service.lock").open("a+b") as lock:
            lock.seek(0)
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
            stop = self.state_dir / "stop"
            stop.unlink(missing_ok=True)
            tasks = []
            try:
                await self.start_pool()
                server = await self.tcp_endpoint()
                tasks.append(asyncio.create_task(self.v1_scan(self.options.local_queue)))
                if self.options.hosts:
                    from peers import PeerManager
                    self.peers = PeerManager(self, self.options.hosts, {"v1_queue": self.options.relay_v1_queue,
                                                                        "socket": self.options.relay_socket})
                    tasks.append(asyncio.create_task(self.peers.run()))
                self.log_event("started", pid=os.getpid(), workers=[s.name + ":" + s.state for s in self.slots])
                self.note_build()
                while not stop.exists():
                    self.write_status()
                    self.evict()
                    self.schedule_fresh()
                    await asyncio.sleep(0.5)
            finally:
                self.stopping = True
                for t in tasks:
                    t.cancel()
                for conn in list(self.conns):
                    conn.close()
                await asyncio.sleep(0.2)
                for slot in self.slots:
                    await self.loop.run_in_executor(self.executor, slot.proc.stop)
                self.log_event("stopped", pid=os.getpid())
                self.batch_log.close()
                status = {"pid": os.getpid(), "running": False, "utc": logs.utc().isoformat(timespec="seconds")}
                (self.state_dir / "status.json").write_text(json.dumps(status), encoding="ascii")


def command_start(options):
    """Launch the broker detached, on its own private desktop (never on the owner's desktop)."""
    state = options.state
    state.mkdir(parents=True, exist_ok=True)
    for stale in ("status.json", "endpoint.json"):
        (state / stale).unlink(missing_ok=True)
    guard = HERE.parents[1] / "excel-session" / "on-private-desktop.ps1"
    run = f'py -3 "{HERE / "broker.py"}" run --state "{state}" --workers {options.workers} --local-queue "{options.local_queue}"'
    if options.hosts:
        run += " --hosts " + " ".join(options.hosts) + f' --relay-v1-queue "{options.relay_v1_queue}" --relay-socket "{options.relay_socket}"'
    run += f" --desktop {options.desktop}"
    if options.admission:
        run += f" --admission {options.admission}"
    if options.registry is not None:
        run += " --registry " + " ".join(options.registry)
    run += f" --leak-every {options.leak_every}"
    subprocess.Popen(["pwsh", "-NoProfile", "-File", str(guard), "-Command", run, "-Log", str(state / "console.log"),
                      "-GuardOut", str(state / "guard.json"), "-SessionRoot", str(state / "excel"),
                      "-DesktopName", options.desktop],
                     stdin=subprocess.DEVNULL, stdout=open(state / "guard.out", "ab"), stderr=subprocess.STDOUT,
                     creationflags=subprocess.CREATE_NO_WINDOW | subprocess.CREATE_NEW_PROCESS_GROUP)
    deadline = time.monotonic() + 120
    status = state / "status.json"
    while time.monotonic() < deadline:
        if status.exists():
            data = json.loads(status.read_text())
            if data.get("running") and any(w["state"] != "new" for w in data.get("workers", [])):
                print("touchstone broker running:", json.dumps(data["workers"]))
                return
        time.sleep(0.5)
    raise TimeoutError("broker start not confirmed; see " + str(state / "console.log"))


def command_stop(options):
    import msvcrt
    (options.state / "stop").touch()
    deadline = time.monotonic() + 150
    with (options.state / "service.lock").open("a+b") as lock:
        while True:
            lock.seek(0)
            try:
                msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
            except OSError:
                if time.monotonic() > deadline:
                    raise TimeoutError("stop not confirmed; no process was killed")
                time.sleep(0.2)
            else:
                msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)
                break
    print("touchstone broker stopped")


def parse_since(text):
    if not text:
        return None
    from datetime import datetime, timedelta, timezone
    if text.endswith("m") and text[:-1].isdigit():
        return datetime.now(timezone.utc) - timedelta(minutes=int(text[:-1]))
    if text.endswith("h") and text[:-1].isdigit():
        return datetime.now(timezone.utc) - timedelta(hours=int(text[:-1]))
    if text.endswith("d") and text[:-1].isdigit():
        return datetime.now(timezone.utc) - timedelta(days=int(text[:-1]))
    return datetime.fromisoformat(text).replace(tzinfo=timezone.utc)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["start", "stop", "run", "status", "stats", "waits"])
    parser.add_argument("--state", type=Path, default=DEFAULT_STATE)
    parser.add_argument("--workers", type=int, default=3)
    parser.add_argument("--local-queue", type=Path, default=None,
                        help="the v1 file queue to serve (default: queue under --state; ~/excel-queue only after the v1 service is stopped)")
    parser.add_argument("--hosts", nargs="*", default=[])
    parser.add_argument("--relay-v1-queue", default="~/excel-queue-v2",
                        help="the v1 file queue each relay serves on its host (~/excel-queue at cutover)")
    parser.add_argument("--relay-socket", default="~/.excel-service/excel.sock")
    parser.add_argument("--registry", nargs="*", default=None,
                        help="functions served in serial mode (default WEEKNUM; 'none' for an empty registry, to test the leak check)")
    parser.add_argument("--leak-every", type=int, default=100, help="reversed-order check on one batch in this many")
    parser.add_argument("--admission", default=None, help="module:factory of the admission plug-in")
    parser.add_argument("--desktop", default="DnaCalcExcelV2")
    parser.add_argument("--since", default=None, help="e.g. 24h, 7d or an ISO time")
    options = parser.parse_args()
    options.local_queue = options.local_queue or options.state / "queue"
    if options.command == "run":
        asyncio.run(Broker(options).run())
    elif options.command == "start":
        command_start(options)
    elif options.command == "stop":
        command_stop(options)
    elif options.command == "status":
        print((options.state / "status.json").read_text())
    elif options.command == "stats":
        print(json.dumps(logs.stats(options.state / "log", parse_since(options.since)), indent=1))
    elif options.command == "waits":
        print(json.dumps(logs.WaitsLog.summary(options.state / "log" / "waits.jsonl", parse_since(options.since)), indent=1))


if __name__ == "__main__":
    main()
