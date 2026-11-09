"""touchstone/1 client connection (standard library only; blocking sockets plus one reader thread).

    conn = Connection.open()                 # unix socket if present, else the broker's local TCP endpoint
    pending = conn.submit_groups(groups)     # or conn.submit_rows(function, rows) with typed (v1 style) rows
    pending.result(timeout=30)               # -> v1-shaped reply dict

Requests are resent after a reconnect with the same ids; the service never evaluates a piece twice.
"""
import json
import os
from pathlib import Path
import socket
import threading
import time
import uuid

try:
    from . import protocol, v1
except ImportError:
    import protocol
    import v1

DEFAULT_SOCKET = Path.home() / ".excel-service" / "excel.sock"
DEFAULT_ENDPOINT = Path(r"C:\Work\DnaCalc\codex-runs\excel-service-v2") / "endpoint.json"


class ServiceError(RuntimeError):
    def __init__(self, response):
        self.response = response
        super().__init__(response.get("error", "service error"))


class Pending:
    """One request in flight: assembles answer frames into positions, resolves on DONE or ERROR."""

    def __init__(self, rid, calls, groups, opts, envs, machine, rows=None, function=None, result_index=None):
        self.rid, self.calls, self.groups, self.opts = rid, calls, groups, opts
        self.envs, self.machine = envs, machine
        self.event = threading.Event()
        self.letters = [None] * calls
        self.values = [None] * calls
        self.inputs = {}
        self.error = None
        self.env_ids = set()
        self.env_by_position = [None] * calls
        self.t_submit = time.monotonic()
        self.t_done = None
        self.rows, self.function, self.index = rows, function, result_index
        self.order = None
        self.frames_sent = []
        self.callbacks = []

    def on_answer(self, frame):
        for g in frame["g"]:
            positions = range(g["o"], g["o"] + len(g["ty"])) if "o" in g else g["i"]
            for p, l, x in zip(positions, g["ty"], g["v"]):
                self.env_by_position[p] = frame.get("env")
                self.letters[p] = l
                self.values[p] = x
            if "ity" in g:
                width = len(g["ity"]) // max(1, len(g["ty"]))
                for n, p in enumerate(positions):
                    self.inputs[p] = list(zip(g["ity"][n * width:(n + 1) * width], g["iv"][n * width:(n + 1) * width]))
        if frame.get("env"):
            self.env_ids.add(frame["env"])
        if frame.get("order"):
            self.order = frame["order"]

    def resolve(self, error=None):
        if error and not self.error:
            self.error = error
        self.t_done = time.monotonic()
        self.event.set()
        for callback in self.callbacks:
            callback(self)

    @property
    def seconds(self):
        return (self.t_done or time.monotonic()) - self.t_submit

    def done(self):
        return self.event.is_set()

    def raw(self, timeout=None):
        """(letters, values) per position; raises on error or timeout."""
        if not self.event.wait(timeout):
            raise TimeoutError(f"Excel question {self.rid} timed out")
        if self.error:
            raise ServiceError({"id": self.rid, "ok": False, "error": self.error})
        return self.letters, self.values

    def result(self, timeout=None):
        """The v1-shaped reply (typed rows, inputs when requested, environment fields)."""
        letters, values = self.raw(timeout)
        if self.rows is None:
            raise RuntimeError("this request was submitted as groups; use raw()")
        reply = v1.Reply(self.rid, self.function, self.rows, self.index, None, self.envs, self.machine)
        for p, (l, x) in enumerate(zip(letters, values)):
            reply.values[p] = protocol.answer_to_typed(l, x)
            if p in self.inputs:
                reply.inputs[p] = [protocol.answer_to_typed(il, ix) for il, ix in self.inputs[p]]
        reply.env_id = sorted(self.env_ids)[0] if self.env_ids else None
        built = reply.build(strict=False)
        if not built["ok"]:
            raise ServiceError(built)
        built["service_seconds"] = self.seconds
        if self.order:
            built["order"] = self.order
        return built


class Connection:
    """One connection to a relay socket or the broker's local endpoint.  When it drops (the ssh session was
    cut, the broker restarted) it reconnects with backoff and resends every unanswered request under the
    same ids; the broker answers resent pieces from its cache and never evaluates one twice."""

    def __init__(self, opener, name="client", reconnect=True):
        self.opener = opener
        self.name = name
        self.reconnect = reconnect
        self.envs = {}
        self.machine = None
        self.pending = {}
        self.lock = threading.Lock()
        self.write_lock = threading.Lock()
        self.alive = False
        self.closed = False
        self.reconnects = 0
        self.sock = None
        self.hello = threading.Event()
        delay = 0.25
        for attempt in range(12):       # the relay may be restarting (its ssh session was just replaced)
            try:
                self._connect(first=True)
                break
            except (OSError, ValueError):
                if attempt == 11:
                    raise
                time.sleep(delay)
                delay = min(3.0, delay * 1.7)
        self.thread = threading.Thread(target=self._read_loop, daemon=True)
        self.thread.start()

    # -- opening ------------------------------------------------------------------------
    @classmethod
    def open(cls, socket_path=None, endpoint=None, name="client", reconnect=True):
        socket_path = Path(socket_path or os.environ.get("TOUCHSTONE_SOCKET") or DEFAULT_SOCKET)
        endpoint = Path(endpoint or os.environ.get("TOUCHSTONE_ENDPOINT") or DEFAULT_ENDPOINT)

        def opener():
            if hasattr(socket, "AF_UNIX") and socket_path.exists():
                sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                sock.connect(str(socket_path))
                return sock, None
            info = json.loads(endpoint.read_text())
            sock = socket.create_connection((info["host"], info["port"]), timeout=10)
            sock.settimeout(None)
            sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            return sock, info["token"]

        conn = cls(opener, name, reconnect)
        conn.wait_hello()
        return conn

    def _connect(self, first=False):
        sock, token = self.opener()
        self.sock = sock
        self.reader = protocol.FrameReader()
        self.hello.clear()
        self.alive = True
        self._send(protocol.HELLO, {"v": protocol.VERSION, "name": self.name, "token": token})

    # -- plumbing -----------------------------------------------------------------------
    def _send(self, kind, obj):
        data = protocol.pack(kind, obj)
        with self.write_lock:
            self.sock.sendall(data)

    def _read_loop(self):
        backoff = 0.5
        while not self.closed:
            try:
                while True:
                    data = self.sock.recv(1 << 20)
                    if not data:
                        break
                    backoff = 0.5
                    for kind, body in self.reader.feed(data):
                        self._on_frame(kind, body)
            except (OSError, ValueError):
                pass
            self.alive = False
            if self.closed:
                break
            if not self.reconnect:
                break
            while not self.closed:
                time.sleep(backoff)
                backoff = min(30.0, backoff * 2)
                try:
                    self._connect()
                except (OSError, ValueError):
                    continue
                self.reconnects += 1
                self._resend()
                break
        with self.lock:
            waiting = list(self.pending.values())
            self.pending.clear()
        for pending in waiting:
            pending.resolve("connection lost")
        self.hello.set()

    def _resend(self):
        with self.lock:
            waiting = list(self.pending.values())
        for pending in waiting:
            try:
                for frame in pending.frames_sent:
                    self._send(protocol.SUBMIT, frame)
            except OSError:
                return

    def _on_frame(self, kind, body):
        if kind == protocol.HELLO:
            self.machine = body.get("server")
            self.envs.update({k: {"env": v} for k, v in body.get("envs", {}).items()})
            self.hello.set()
        elif kind == protocol.ENV:
            self.envs[body["env_id"]] = {"env": body["env"]}
        elif kind == protocol.ANSWER:
            pending = self.pending.get(body["id"])
            if pending:
                pending.on_answer(body)
        elif kind == protocol.DONE:
            with self.lock:
                pending = self.pending.pop(body["id"], None)
            if pending:
                pending.resolve()
        elif kind == protocol.ERROR:
            rid = body.get("id")
            with self.lock:
                pending = self.pending.pop(rid, None)
            if pending:
                pending.resolve(f"{body.get('code')}: {body.get('detail')}")
            elif rid is None:
                with self.lock:
                    waiting = list(self.pending.values())
                    self.pending.clear()
                for p in waiting:
                    p.resolve(f"{body.get('code')}: {body.get('detail')}")

    def wait_hello(self, timeout=10):
        if not self.hello.wait(timeout):
            raise ConnectionError("no hello from the service")

    # -- requests -----------------------------------------------------------------------
    def submit_groups(self, groups, *, priority=None, echo=False, order="any", rid=None):
        rid = rid or uuid.uuid4().hex
        calls = sum(g["n"] for g in groups)
        groups = protocol.with_positions(groups) if any("o" not in g and "i" not in g for g in groups) else groups
        opts = {"echo": echo, "order": order, "calls": calls}
        if priority:
            opts["priority"] = priority
        pending = Pending(rid, calls, groups, opts, self.envs, self.machine)
        self._submit(pending, protocol.submit_frames(rid, groups, opts))
        return pending

    def submit_rows(self, function, rows, *, result_index=None, priority=None, echo=False, order="any", rid=None,
                    spill=False, primer=None):
        """primer: [(function, typed_row), ...] evaluated first, in order, in serial mode (answers dropped)."""
        rid = rid or uuid.uuid4().hex
        groups = protocol.rows_to_groups(function, rows, result_index, spill)
        opts = {"echo": echo, "order": order, "calls": len(rows)}
        if primer:
            opts["primer"] = [protocol.rows_to_groups(f, [row])[0] for f, row in primer]
        if priority:
            opts["priority"] = priority
        pending = Pending(rid, len(rows), groups, opts, self.envs, self.machine, rows=rows, function=function, result_index=result_index)
        self._submit(pending, protocol.submit_frames(rid, groups, opts))
        return pending

    def _submit(self, pending, frames):
        pending.frames_sent = frames
        with self.lock:
            self.pending[pending.rid] = pending
        if not self.alive:
            return      # sent after the reconnect, with everything else that is unanswered
        try:
            for frame in frames:
                self._send(protocol.SUBMIT, frame)
        except OSError:
            pass        # the reader notices the lost connection and resends

    def report_wait(self, reason, seconds, kind="client-timeout", lane=None):
        self._send(protocol.WAIT, {"kind": kind, "reason": reason, "seconds": seconds, "lane": lane})

    def close(self):
        self.closed = True
        try:
            self.sock.close()
        except OSError:
            pass
