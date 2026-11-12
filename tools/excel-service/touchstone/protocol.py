"""touchstone/1 wire format. Python standard library only; no DnaCalc imports.

A call is a function name plus typed arguments.  Calls with one function and one argument
shape travel as a *group* (columnar, so thousands of calls cost one small header):

    {"f": "ERF.PRECISE", "k": "n", "n": 3, "v": [...3 x width values...], "x": [r, c]?, "i": [...]}

``k`` lists one token per argument, comma separated:
    n  number (value: 16 hex digits, the binary64 bits)
    s  text through a cell        S  text as a formula literal
    b  boolean through a cell     B  boolean as a formula literal
    e  worksheet error (value: "#N/A" ...)
    z  blank (value: null)
    aRxC  numeric array of R rows and C columns, value: R*C hex strings inline
``x`` is an optional [row, column] result selector (regression functions).  Each group says where its
calls sit in the request: ``o`` (first position, calls contiguous) or ``i`` (the list of positions).

An answer group is {"ty": "nntb...", "v": [...]} with one type letter and value per call:
    n hex bits   t text   b boolean   e "#N/A"   z null   u {"reason":..., "read_back":...}
Frames: 4-byte big-endian length, then a type byte and a JSON body; a type byte >= 0x80
marks a zlib-compressed body.
"""
import json
import re
import struct
import zlib

VERSION = "touchstone/1"
HEX16 = re.compile(r"[0-9a-f]{16}")
ERROR_CODES = {"#NULL!": 2000, "#DIV/0!": 2007, "#VALUE!": 2015, "#REF!": 2023,
               "#NAME?": 2029, "#NUM!": 2036, "#N/A": 2042}
CODE_NAMES = {v: k for k, v in ERROR_CODES.items()}
MAX_FRAME = 64 * 1024

# frame types
HELLO, SUBMIT, ANSWER, DONE, ENV, ERROR, PING, PONG, CANCEL, WAIT = (bytes([c]) for c in b"HSADEXPQCW")


def dumps(obj):
    return json.dumps(obj, ensure_ascii=True, separators=(",", ":"), allow_nan=False).encode("ascii")


def pack(kind, obj, compress=False):
    body = dumps(obj)
    if compress and len(body) > 200:
        body = zlib.compress(body, 1)
        kind = bytes([kind[0] | 0x80])
    return struct.pack(">I", len(body) + 1) + kind + body


def unpack_body(data):
    kind = data[0]
    body = data[1:]
    if kind & 0x80:
        body = zlib.decompress(body)
        kind &= 0x7f
    return bytes([kind]), json.loads(body)


class FrameReader:
    """Incremental frame parser for a byte stream."""

    def __init__(self):
        self.buffer = bytearray()

    def feed(self, data):
        self.buffer += data
        while len(self.buffer) >= 4:
            (size,) = struct.unpack_from(">I", self.buffer)
            if size > 64 * 1024 * 1024:
                raise ValueError("frame too large")
            if len(self.buffer) < 4 + size:
                break
            frame = bytes(self.buffer[4:4 + size])
            del self.buffer[:4 + size]
            yield unpack_body(frame)


# --- typed (v1 style) <-> columnar -------------------------------------------------------

def _token(arg):
    kind = arg["type"]
    literal = arg.get("mode", "cell") == "literal"
    if kind == "number":
        return "n", arg["bits"]
    if kind == "text":
        return ("S" if literal else "s"), arg["value"]
    if kind == "boolean":
        return ("B" if literal else "b"), arg["value"]
    if kind == "error":
        return "e", arg["code"]
    if kind == "blank":
        return "z", None
    if kind == "array":
        rows = arg["values"]
        flat = [v["bits"] for r in rows for v in r]
        return f"a{len(rows)}x{len(rows[0])}", flat
    if kind == "range":
        rows = arg["values"]
        return f"r{len(rows)}x{len(rows[0])}", [v for row in rows for v in row]
    if kind == "file":
        return "f", arg["content"]
    raise ValueError("unsupported argument type " + str(kind))


def rows_to_groups(function, rows, result_index=None, spill=False):
    """Typed rows (the v1 client shape) -> columnar groups, preserving call positions.

    spill=True asks for the whole array an array-returning function spills (each answer is a matrix)."""
    groups = {}
    for position, row in enumerate(rows):
        tokens, values = [], []
        for arg in row:
            token, value = _token(arg)
            tokens.append(token)
            if isinstance(value, list):
                values.extend(value)
            else:
                values.append(value)
        key = ",".join(tokens)
        group = groups.get(key)
        if group is None:
            group = groups[key] = {"f": function, "k": key, "n": 0, "v": [], "i": []}
            if result_index is not None:
                group["x"] = list(result_index)
            if spill:
                group["spill"] = True
        group["n"] += 1
        group["v"].extend(values)
        group["i"].append(position)
    result = list(groups.values())
    for group in result:
        if group["i"] == list(range(group["i"][0], group["i"][0] + group["n"])):
            group["o"] = group.pop("i")[0]
    return result


def positions(group):
    """The request positions of a group's calls."""
    if "o" in group:
        return range(group["o"], group["o"] + group["n"])
    return group["i"]


def token_width(token):
    if token[0] in "ar":
        r, c = token[1:].split("x")
        return int(r) * int(c)
    return 1


def group_width(group):
    return sum(token_width(t) for t in group["k"].split(",")) if group["k"] else 0


def group_calls(group):
    """Yield each call of a group as a list of (token, value) typed pieces."""
    tokens = group["k"].split(",") if group["k"] else []
    widths = [token_width(t) for t in tokens]
    v = group["v"]
    offset = 0
    for _ in range(group["n"]):
        call = []
        for token, width in zip(tokens, widths):
            call.append((token, v[offset:offset + width] if token[0] in "ar" else v[offset]))
            offset += width
        yield call


def group_to_rows(group):
    """Columnar group -> typed rows (for the v1 shim, tests and policy checks)."""
    rows = []
    for call in group_calls(group):
        row = []
        for token, value in call:
            if token == "n":
                row.append({"type": "number", "bits": value})
            elif token in ("s", "S"):
                row.append({"type": "text", "value": value, "mode": "literal" if token == "S" else "cell"})
            elif token in ("b", "B"):
                row.append({"type": "boolean", "value": value, "mode": "literal" if token == "B" else "cell"})
            elif token == "e":
                row.append({"type": "error", "code": value})
            elif token == "z":
                row.append({"type": "blank"})
            elif token == "f":
                row.append({"type": "file", "content": value})
            elif token[0] == "r":
                r, c = (int(x) for x in token[1:].split("x"))
                row.append({"type": "range", "values": [value[a * c:(a + 1) * c] for a in range(r)]})
            else:
                r, c = (int(x) for x in token[1:].split("x"))
                row.append({"type": "array", "values": [
                    [{"type": "number", "bits": value[a * c + b]} for b in range(c)] for a in range(r)]})
        rows.append(row)
    return rows


def typed_to_answer(typed):
    """A v1 typed answer {"type": "number", "bits": ...} -> (type letter, value)."""
    kind = typed["type"]
    if kind == "number":
        return "n", typed["bits"]
    if kind == "text":
        return "t", typed["value"]
    if kind == "boolean":
        return "b", typed["value"]
    if kind == "error":
        return "e", typed["code"]
    if kind == "blank":
        return "z", None
    raise ValueError(kind)


def answer_to_typed(letter, value):
    if letter == "n":
        return {"type": "number", "bits": value}
    if letter == "t":
        return {"type": "text", "value": value}
    if letter == "b":
        return {"type": "boolean", "value": value}
    if letter == "e":
        return {"type": "error", "code": value}
    if letter == "z":
        return {"type": "blank"}
    if letter == "u":
        return {"type": "unread", **value}
    if letter == "m":
        r, c = value["r"], value["c"]
        cells = [answer_to_typed(l, x) for l, x in zip(value["ty"], value["v"])]
        return {"type": "array", "rows": r, "cols": c, "values": [cells[i * c:(i + 1) * c] for i in range(r)]}
    raise ValueError(letter)


def split_group(group, max_calls):
    """Cut a group into pieces of at most max_calls calls (positions stay attached)."""
    if group["n"] <= max_calls:
        return [group]
    width = group_width(group)
    pieces = []
    for start in range(0, group["n"], max_calls):
        n = min(max_calls, group["n"] - start)
        piece = {"f": group["f"], "k": group["k"], "n": n,
                 "v": group["v"][start * width:(start + n) * width]}
        for key in ("x", "spill", "p"):
            if key in group:
                piece[key] = group[key]
        if "o" in group:
            piece["o"] = group["o"] + start
        else:
            piece["i"] = group["i"][start:start + n]
        pieces.append(piece)
    return pieces


def with_positions(groups, start=0):
    """Give position-less groups consecutive request positions (a copy; the input is untouched)."""
    result = []
    for group in groups:
        g = dict(group)
        g.pop("i", None)
        g["o"] = start
        start += g["n"]
        result.append(g)
    return result


FRAME_BUDGET = 40 * 1024


def submit_frames(rid, groups, opts=None, client=None):
    """Cut groups into submit frames of about FRAME_BUDGET bytes: [{id, seq, last, groups, opts?, c?}]."""
    frames, current, size = [], [], 0
    for group in groups:
        per_call = max(8, group_width(group) * 20)
        step = max(1, FRAME_BUDGET // per_call)
        pending = split_group(group, step)
        while pending:
            part = pending.pop(0)
            encoded = len(dumps(part))
            if encoded > FRAME_BUDGET:
                if part["n"] == 1:
                    raise ValueError("one call exceeds the frame budget")
                pending[0:0] = split_group(part, max(1, part["n"] // 2))
                continue
            if current and size + encoded > FRAME_BUDGET:
                frames.append(current)
                current, size = [], 0
            current.append(part)
            size += encoded
            if size >= FRAME_BUDGET:
                frames.append(current)
                current, size = [], 0
    if current or not frames:
        frames.append(current)
    result = []
    for seq, in_frame in enumerate(frames):
        frame = {"id": rid, "seq": seq, "last": seq == len(frames) - 1, "groups": in_frame}
        if seq == 0 and opts:
            frame["opts"] = opts
        if client is not None:
            frame["c"] = client
        result.append(frame)
    return result
