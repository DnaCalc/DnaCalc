"""Typed Excel questions through a local file queue. Python standard library only."""
import json
import math
import os
from pathlib import Path
import struct
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


class Client:
    def __init__(self, queue=None, timeout=180):
        self.queue = Path(queue or Path.home() / "excel-queue").expanduser()
        self.timeout = timeout

    def ask(self, function, rows, *, result_index=None):
        """Return {rows: typed answers, ...provenance}; failures raise ServiceError.

        Request and response files remain available after a timeout. Retrying ask creates
        a new question; the service never silently substitutes or rounds an input.
        """
        if not math.isfinite(self.timeout) or not 0 < self.timeout <= 900:
            raise ValueError("timeout must be between 0 and 900 seconds")
        request_id = uuid.uuid4().hex
        request = {"version": 1, "id": request_id, "function": function, "rows": rows,
                   "created": time.time(), "expires": time.time() + self.timeout}
        if result_index is not None:
            request["result_index"] = list(result_index)
        output = self.queue / "out" / (request_id + ".json")
        atomic_json(self.queue / "in" / (request_id + ".json"), request)
        deadline = time.monotonic() + self.timeout
        while not output.exists():
            if time.monotonic() >= deadline:
                raise TimeoutError(f"Excel question {request_id} timed out; request retained in {self.queue}")
            time.sleep(0.05)
        response = json.loads(output.read_text(encoding="utf-8"))
        if response.get("id") != request_id or response.get("version") != 1:
            raise ValueError("Mismatched Excel response identity/version")
        if not response.get("ok"):
            raise ServiceError(response)
        if len(response["rows"]) != len(rows):
            raise ValueError("Incomplete Excel response")
        return response

    def consecutive_run(self, function, start, count, *, arguments=(), index=0):
        """Ask consecutive positive finite binary64 encodings in one batch."""
        first = int(number(start)["bits"], 16)
        if not 1 <= count <= 20000 or first + count - 1 >= 0x7ff0000000000000:
            raise ValueError("run must contain 1..20000 nonnegative finite doubles")
        rows = []
        for value in range(first, first + count):
            row = list(arguments)
            row.insert(index, number(f"{value:016x}"))
            rows.append(row)
        return self.ask(function, rows)

    def bisect_edge(self, function, low, high, predicate, *, arguments=(), index=0):
        """Find adjacent nonnegative binary64 inputs where a monotone predicate flips.

        predicate(input_bits, typed_answer) executes only on the client. Endpoints
        must differ; the caller is responsible for monotonicity of the predicate.
        Return None when a reading cannot distinguish the sides; nearby inputs are
        then queried without guessing a side. Every question is retained by ask().
        """
        lo, hi = (int(number(x)["bits"], 16) for x in (low, high))
        if not 0 <= lo < hi < 0x7ff0000000000000:
            raise ValueError("expected increasing nonnegative finite doubles")

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
            middle = (lo + hi) // 2
            answer = query([middle])[0]
            questions += 1
            outcome = predicate(f"{middle:016x}", answer)
            candidates = [(middle, answer, outcome)]
            if outcome is None:
                for radius in (4, 16, 64):
                    nearby = [x for x in range(max(lo + 1, middle - radius), min(hi, middle + radius + 1)) if x != middle]
                    if not nearby:
                        raise ValueError("readings cannot locate the edge to adjacent inputs")
                    answers = query(nearby)
                    questions += 1
                    candidates = [(x, y, predicate(f"{x:016x}", y)) for x, y in zip(nearby, answers)]
                    if any(p is not None for _, _, p in candidates):
                        break
                else:
                    raise ValueError("no distinguishing reading near midpoint; narrow the bracket")
            for value, answer, outcome in candidates:
                if outcome is not None and lo < value < hi:
                    if bool(outcome) == side:
                        lo, left = value, answer
                    else:
                        hi, right = value, answer
        return {"low": f"{lo:016x}", "high": f"{hi:016x}",
                "low_answer": left, "high_answer": right, "questions": questions}


def ask(function, rows, **options):
    return Client(**options).ask(function, rows)


def bisect_edge(function, low, high, predicate, **options):
    return Client().bisect_edge(function, low, high, predicate, **options)


def consecutive_run(function, start, count, **options):
    return Client().consecutive_run(function, start, count, **options)
