"""Typed Excel questions. Python standard library only.

Transport: the touchstone/1 socket when there is one (the relay's ~/.excel-service/excel.sock on a Linux lane
host, the broker's local endpoint on the Windows machine) and the touchstone package is beside this file;
otherwise the v1 file queue, which the relay and the broker also serve. A Client(queue=...) always uses the
file queue. ask, consecutive_run and bisect_edge keep their signatures; submit() returns a Future, ask_many and
stream() send many requests at once, AsyncClient offers await ask and as_completed.
"""
import concurrent.futures
import json
import math
import os
from pathlib import Path
import struct
import sys
import time
import uuid


def bits(value):
    return struct.pack(">d", value).hex()


def from_bits(value):
    return struct.unpack(">d", bytes.fromhex(value.removeprefix("0x")))[0]


def number(value):
    """A float or its exact 16-digit binary64 encoding (not float.hex notation)."""
    return {"type": "number", "bits": bits(value) if not isinstance(value, str)
            else value.removeprefix("0x").lower()}


def number_array(rows):
    """A rectangular numeric array, transported as individual Value2 cells."""
    return {"type": "array", "values": [[number(v) for v in row] for row in rows]}


def text(value, *, literal=False):
    return {"type": "text", "value": value, "mode": "literal" if literal else "cell"}


def boolean(value, *, literal=False):
    return {"type": "boolean", "value": value, "mode": "literal" if literal else "cell"}


def blank():
    return {"type": "blank"}


def error(code):
    return {"type": "error", "code": code}


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix("." + uuid.uuid4().hex + ".tmp")
    with temporary.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=True, allow_nan=False, separators=(",", ":"))
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


class ServiceError(RuntimeError):
    def __init__(self, response):
        self.response = response
        super().__init__(response["error"])


_HERE = Path(__file__).resolve().parent


def _touchstone():
    """The touchstone client module, or None when this file was copied on its own."""
    try:
        if str(_HERE) not in sys.path:
            sys.path.insert(0, str(_HERE))
        from touchstone import client as module
        return module
    except ImportError:
        return None


class Client:
    def __init__(self, queue=None, timeout=180, *, socket_path=None, endpoint=None, transport="auto"):
        self.explicit_queue = queue is not None
        self.queue = Path(queue or Path.home() / "excel-queue").expanduser()
        self.timeout = timeout
        self.socket_path, self.endpoint = socket_path, endpoint
        self.transport = "file" if (self.explicit_queue and transport == "auto") else transport
        self._connection = None
        self._pool = None

    # -- transport ----------------------------------------------------------------------
    def _socket(self):
        """The open touchstone connection, or None when this client uses the file queue."""
        if self.transport == "file":
            return None
        if self._connection is not None:
            return self._connection
        module = _touchstone()
        if module is None:
            if self.transport == "socket":
                raise RuntimeError("the touchstone package is not beside client.py")
            return None
        try:
            self._connection = module.Connection.open(self.socket_path, self.endpoint, name="py-client")
        except (OSError, ValueError, ConnectionError, KeyError):
            if self.transport == "socket":
                raise
            return None
        return self._connection

    @property
    def uses_socket(self):
        return self._socket() is not None

    def _executor(self):
        if self._pool is None:
            self._pool = concurrent.futures.ThreadPoolExecutor(max_workers=16, thread_name_prefix="excel-file-queue")
        return self._pool

    def close(self):
        if self._connection is not None:
            self._connection.close()
            self._connection = None
        if self._pool is not None:
            self._pool.shutdown(wait=False)
            self._pool = None

    # -- one request --------------------------------------------------------------------
    def _check_timeout(self):
        if not math.isfinite(self.timeout) or not 0 < self.timeout <= 900:
            raise ValueError("timeout must be between 0 and 900 seconds")

    def submit(self, function, rows, *, result_index=None, priority=None, order="any", echo_inputs=False,
               shape="scalar", primer=None):
        """Send a question; returns a concurrent.futures.Future of the reply (ServiceError on failure).

        priority: "interactive" or "bulk" (default: up to 100 calls are interactive); order "any" or "serial"
        (state-sensitive functions); echo_inputs asks for the exact inputs Excel read back.  The file queue
        has one lane and always echoes inputs; the options apply to the socket transport.

        order="serial" evaluates the calls one at a time, in the order given, on the serial worker (for functions
        whose answer depends on the call before it; WEEKNUM is always served this way); primer is a list of
        (function, row) calls evaluated first to put the engine in a known state.  shape="array" asks for the whole
        array an array-returning function spills: each answer row is {"type": "array", "rows", "cols", "values"}.
        """
        self._check_timeout()
        if order not in ("any", "serial"):
            raise ValueError("order must be 'any' or 'serial'")
        connection = self._socket()
        if connection is None:
            if shape != "scalar" or order != "any" or primer:
                raise RuntimeError("array answers and serial mode need the touchstone socket, not the v1 file queue")
            return self._executor().submit(self._ask_file, function, rows, result_index)
        future = concurrent.futures.Future()
        pending = connection.submit_rows(function, rows, result_index=result_index, priority=priority,
                                         echo=echo_inputs, order=order, spill=(shape == "array"), primer=primer)

        def settle(done):
            try:
                if done.error:
                    future.set_exception(ServiceError({"id": done.rid, "ok": False, "error": done.error}))
                else:
                    future.set_result(done.result())
            except Exception as failure:
                if not future.done():
                    reply = getattr(failure, "response", None)
                    future.set_exception(ServiceError(reply) if reply else failure)

        future.request_id = pending.rid
        pending.callbacks.append(settle)
        if pending.done():
            settle(pending)
        return future

    def ask(self, function, rows, *, result_index=None, priority=None, order="any", echo_inputs=False,
            shape="scalar", primer=None):
        """Return {rows: typed answers, ...provenance}; failures raise ServiceError.

        A timeout raises TimeoutError; the request stays known to the service, and retrying ask creates a new
        question.  The service never silently substitutes or rounds an input.
        """
        self._check_timeout()
        if self._socket() is None and shape == "scalar" and order == "any" and not primer:
            return self._ask_file(function, rows, result_index)
        future = self.submit(function, rows, result_index=result_index, priority=priority, order=order,
                             echo_inputs=echo_inputs, shape=shape, primer=primer)
        try:
            return future.result(self.timeout)
        except concurrent.futures.TimeoutError:
            raise TimeoutError(f"Excel question {getattr(future, 'request_id', '?')} timed out") from None

    def _ask_file(self, function, rows, result_index=None):
        self._check_timeout()
        request_id = uuid.uuid4().hex
        request = {"version": 1, "id": request_id, "function": function, "rows": rows,
                   "created": time.time(), "expires": time.time() + self.timeout}
        if result_index is not None:
            request["result_index"] = list(result_index)
        output = self.queue / "out" / (request_id + ".json")
        atomic_json(self.queue / "in" / (request_id + ".json"), request)
        deadline = time.monotonic() + self.timeout
        delay = 0.005
        while not output.exists():
            if time.monotonic() >= deadline:
                raise TimeoutError(f"Excel question {request_id} timed out; request retained in {self.queue}")
            time.sleep(delay)
            delay = min(0.05, delay * 1.5)
        response = json.loads(output.read_text(encoding="utf-8"))
        if response.get("id") != request_id or response.get("version") != 1:
            raise ValueError("Mismatched Excel response identity/version")
        if not response.get("ok"):
            raise ServiceError(response)
        if len(response["rows"]) != len(rows):
            raise ValueError("Incomplete Excel response")
        return response

    # -- many requests ------------------------------------------------------------------
    @staticmethod
    def _spec(request):
        if isinstance(request, dict):
            return request["function"], request["rows"], request.get("result_index"), request.get("priority")
        function, rows, *rest = request
        return function, rows, (rest[0] if rest else None), (rest[1] if len(rest) > 1 else None)

    def ask_many(self, requests, **options):
        """Send every request at once; return the replies in request order (the first failure raises)."""
        futures = [self.submit(f, r, result_index=i, priority=p, **options) for f, r, i, p in map(self._spec, requests)]
        return [self._wait(f) for f in futures]

    def stream(self, requests, **options):
        """Yield (index, reply) as each request completes; a failed request yields (index, ServiceError)."""
        futures = {self.submit(f, r, result_index=i, priority=p, **options): n
                   for n, (f, r, i, p) in enumerate(map(self._spec, requests))}
        for future in concurrent.futures.as_completed(futures, timeout=self.timeout):
            try:
                yield futures[future], future.result()
            except ServiceError as failure:
                yield futures[future], failure

    def _wait(self, future):
        try:
            return future.result(self.timeout)
        except concurrent.futures.TimeoutError:
            raise TimeoutError("Excel question timed out") from None

    # -- helpers built on ask -----------------------------------------------------------
    def consecutive_run(self, function, start, count, *, arguments=(), index=0):
        """Ask consecutive positive finite binary64 encodings in one batch."""
        first = int(number(start)["bits"], 16)
        limit = 1_000_000 if self.uses_socket else 20000
        if not 1 <= count <= limit or first + count - 1 >= 0x7ff0000000000000:
            raise ValueError(f"run must contain 1..{limit} nonnegative finite doubles")
        rows = []
        for value in range(first, first + count):
            row = list(arguments)
            row.insert(index, number(f"{value:016x}"))
            rows.append(row)
        return self.ask(function, rows)

    def bisect_edge(self, function, low, high, predicate, *, arguments=(), index=0, points=None):
        """Find adjacent nonnegative binary64 inputs where a monotone predicate flips.

        predicate(input_bits, typed_answer) executes only on the client. Endpoints must differ; the caller
        is responsible for monotonicity of the predicate.  Each round asks `points` evenly spaced inputs
        (64 on the socket transport, so 52 bits take about nine rounds; 1 on the file queue).  Return None
        from the predicate when a reading cannot distinguish the sides; those readings are skipped, and
        nearby inputs are queried when a whole round is indistinguishable.  Every question is answered by
        the service, never guessed.
        """
        lo, hi = (int(number(x)["bits"], 16) for x in (low, high))
        if not 0 <= lo < hi < 0x7ff0000000000000:
            raise ValueError("expected increasing nonnegative finite doubles")
        points = points or (64 if self.uses_socket else 1)

        def query(values):
            rows = []
            for value in values:
                row = list(arguments)
                row.insert(index, number(f"{value:016x}"))
                rows.append(row)
            return self.ask(function, rows)["rows"]

        left, right = query([lo, hi])
        side = predicate(f"{lo:016x}", left)
        other = predicate(f"{hi:016x}", right)
        if side is None or other is None or bool(side) == bool(other):
            raise ValueError("edge endpoints have the same predicate")
        side = bool(side)
        questions = 1
        while hi - lo > 1:
            if points == 1:
                values = [(lo + hi) // 2]
            else:
                n = min(points, hi - lo - 1)
                values = sorted({lo + (hi - lo) * (k + 1) // (n + 1) for k in range(n)} - {lo, hi})
            answers = query(values)
            questions += 1
            outcomes = [(v, a, predicate(f"{v:016x}", a)) for v, a in zip(values, answers)]
            if all(o is None for _, _, o in outcomes):
                middle = (lo + hi) // 2
                for radius in (4, 16, 64):
                    nearby = [x for x in range(max(lo + 1, middle - radius), min(hi, middle + radius + 1)) if x != middle]
                    if not nearby:
                        raise ValueError("readings cannot locate the edge to adjacent inputs")
                    answers = query(nearby)
                    questions += 1
                    outcomes = [(x, y, predicate(f"{x:016x}", y)) for x, y in zip(nearby, answers)]
                    if any(o is not None for _, _, o in outcomes):
                        break
                else:
                    raise ValueError("no distinguishing reading near midpoint; narrow the bracket")
            for value, answer, outcome in outcomes:
                if outcome is None or not lo < value < hi:
                    continue
                if bool(outcome) == side:
                    lo, left = value, answer
                else:
                    hi, right = value, answer
                    break
        return {"low": f"{lo:016x}", "high": f"{hi:016x}",
                "low_answer": left, "high_answer": right, "questions": questions}


class AsyncClient:
    """asyncio front of Client: await ask(...), await ask_many(...), async for index, reply in as_completed(...)."""

    def __init__(self, *args, **options):
        self.client = Client(*args, **options)

    async def ask(self, function, rows, **options):
        import asyncio
        future = self.client.submit(function, rows, **options)
        return await asyncio.wait_for(asyncio.wrap_future(future), self.client.timeout)

    async def ask_many(self, requests, **options):
        import asyncio
        return await asyncio.gather(*(self.ask(f, r, result_index=i, priority=p, **options)
                                      for f, r, i, p in map(Client._spec, requests)))

    async def as_completed(self, requests, **options):
        """Yield (index, reply) in completion order; a failed request yields (index, ServiceError)."""
        import asyncio
        tasks = {asyncio.ensure_future(self.ask(f, r, result_index=i, priority=p, **options)): n
                 for n, (f, r, i, p) in enumerate(map(Client._spec, requests))}
        pending = set(tasks)
        while pending:
            finished, pending = await asyncio.wait(pending, return_when=asyncio.FIRST_COMPLETED)
            for task in finished:
                try:
                    yield tasks[task], task.result()
                except ServiceError as failure:
                    yield tasks[task], failure

    def close(self):
        self.client.close()


def ask(function, rows, **options):
    """One question through a default Client."""
    client_options = {k: v for k, v in options.items() if k in ("queue", "timeout")}
    return Client(**client_options).ask(function, rows, **{k: v for k, v in options.items() if k not in client_options})


def bisect_edge(function, low, high, predicate, **options):
    return Client().bisect_edge(function, low, high, predicate, **options)


def consecutive_run(function, start, count, **options):
    return Client().consecutive_run(function, start, count, **options)
