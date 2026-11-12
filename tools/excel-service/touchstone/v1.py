"""The v1 file-queue shim: v1 request files -> touchstone submit frames, answer frames -> v1 reply files.

Shared by the Windows broker (its local ~/excel-queue) and the Linux relay (the lane's ~/excel-queue),
so v1 clients and tests keep working unchanged.  No admission here: the broker admits (policy plug-in).
"""
import json
import math
import re
import time

try:
    from . import protocol
except ImportError:  # run as plain modules
    import protocol

ID = re.compile(r"[0-9a-f]{32}")
FRAME_BUDGET = 40 * 1024
MAX_BYTES = 16 * 1024 * 1024


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def parse_request(raw, rid, now=None):
    """Structure, identity and lifetime of a v1 request (the checks of policy.validate that are not
    about the function or its values).  Returns (function, rows, result_index)."""
    if len(raw) > MAX_BYTES:
        raise ValueError("request exceeds 16 MiB")
    request = json.loads(raw, object_pairs_hook=unique_pairs,
                         parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON")))
    if not isinstance(request, dict) or set(request) - {"result_index"} != {"version", "id", "function", "rows", "created", "expires"}:
        raise ValueError("expected only version, id, function, rows, created, expires")
    if type(request["version"]) is not int or request["version"] != 1 or request["id"] != rid:
        raise ValueError("request identity/version mismatch")
    for key in ("created", "expires"):
        if type(request[key]) not in (float, int) or not math.isfinite(request[key]):
            raise ValueError("invalid request timestamp")
    now = time.time() if now is None else now
    if not request["created"] - 60 <= now < request["expires"] or not 0 < request["expires"] - request["created"] <= 901:
        raise ValueError("request expired or invalid lifetime (maximum 900 seconds)")
    if not isinstance(request["function"], str) or not isinstance(request["rows"], list):
        raise ValueError("expected a function name and rows")
    index = request.get("result_index")
    if index is not None and (not isinstance(index, list) or len(index) != 2):
        raise ValueError("result_index must be [row, column]")
    return request["function"], request["rows"], index


def submit_frames(rid, function, rows, result_index=None, *, priority=None, echo=True, order="any"):
    """Cut a v1 request into submit frames of about 40 KB each."""
    groups = protocol.rows_to_groups(function, rows, result_index)
    opts = {"echo": echo, "order": order, "calls": len(rows)}
    if priority:
        opts["priority"] = priority
    return protocol.submit_frames(rid, groups, opts)


class Reply:
    """Collects the answer frames of one request and builds the v1 reply."""

    def __init__(self, rid, request_function, rows, result_index, request_sha256, envs, machine):
        self.rid, self.function, self.rows, self.index = rid, request_function, rows, result_index
        self.sha, self.envs, self.machine = request_sha256, envs, machine
        self.values = [None] * len(rows)
        self.inputs = [None] * len(rows)
        self.env_id = None
        self.error = None
        self.started = time.monotonic()
        self.shape_cache = {}

    def add(self, frame):
        for g in frame["g"]:
            positions = range(g["o"], g["o"] + len(g["ty"])) if "o" in g else g["i"]
            ty, v = g["ty"], g["v"]
            for p, letter, value in zip(positions, ty, v):
                self.values[p] = protocol.answer_to_typed(letter, value)
            if "ity" in g:
                width = len(g["ity"]) // max(1, len(g["ty"]))
                for n, p in enumerate(positions):
                    cells = [protocol.answer_to_typed(l, x) for l, x in zip(g["ity"][n * width:(n + 1) * width], g["iv"][n * width:(n + 1) * width])]
                    self.inputs[p] = cells
        self.env_id = frame.get("env", self.env_id)

    def fail(self, code, detail=None):
        self.error = f"{code}: {detail}" if detail else code

    def build(self, strict=True):
        """The v1 reply dict (ok or failed).  strict: a call Excel could not answer faithfully (an input it changed,
        a spill past the bound) fails the whole reply, as the v1 file queue always did; otherwise that call's row is
        {"type": "unread", "reason": ...} and the rest stands."""
        import socket
        from datetime import datetime, timezone
        stamp = datetime.now(timezone.utc).isoformat()
        if self.error or any(v is None for v in self.values):
            return {"version": 1, "id": self.rid, "ok": False, "error": self.error or "incomplete answer",
                    "rows": [], "excel_build": None, "machine": self.machine or socket.gethostname(), "timestamp": stamp,
                    "request_sha256": self.sha}
        for v in self.values:
            if strict and v["type"] == "unread":
                return {"version": 1, "id": self.rid, "ok": False,
                        "error": "no answer is qualified for a call: " + json.dumps(v),
                        "rows": [], "excel_build": None, "machine": self.machine or socket.gethostname(),
                        "timestamp": stamp, "request_sha256": self.sha}
        env = (self.envs.get(self.env_id) or {}).get("env", {})
        inputs = []
        for row, cells in zip(self.rows, self.inputs):
            if cells is None:
                inputs.append(None)
                continue
            out, at = [], 0
            for arg in row:
                if arg["type"] in ("array", "range"):
                    r, c = len(arg["values"]), len(arg["values"][0])
                    out.append({"type": arg["type"], "values": [cells[at + a * c:at + (a + 1) * c] for a in range(r)]})
                    at += r * c
                elif arg["type"] == "file":
                    out.append({"type": "file", "content": cells[at]["value"]})
                    at += 1
                else:
                    out.append(cells[at])
                    at += 1
            inputs.append(out)
        return {"version": 1, "id": self.rid, "ok": True, "rows": self.values, "inputs": inputs,
                "excel_version": env.get("version"), "excel_build": env.get("build"),
                "excel_file_version": env.get("file_version"), "excel_pid": None,
                "calculation_settings": {
                    "calculation": env.get("calculation"), "compatibility_version": None, "unavailable": [],
                    "PrecisionAsDisplayed": env.get("precision_as_displayed"), "Date1904": env.get("date1904"),
                    "CompatibilityVersion": env.get("compatibility_version"),
                    "UseSystemSeparators": env.get("use_system_separators"),
                    "DecimalSeparator": env.get("decimal_separator"),
                    "ThousandsSeparator": env.get("thousands_separator")},
                "automation_security_before": env.get("automation_security"),
                "automation_security_after": env.get("automation_security"),
                "env_id": self.env_id,
                "machine": self.machine or socket.gethostname(), "timestamp": stamp,
                "service_seconds": time.monotonic() - self.started, "request_sha256": self.sha}
