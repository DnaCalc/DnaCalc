"""A stand-in engine worker for hermetic tests: speaks the worker frame protocol, owns no Excel.

Function behaviour: ID returns each first argument unchanged (as the number it came in); CRASH answers
excel-crashed once per process and then restarts (sends a new hello); SLEEP sleeps 0.3 s per batch; anything
else returns the number 1.0.  CRASHALWAYS answers excel-crashed every time it is asked (and restarts); DIE ends the
worker process without an answer; CRASHON answers excel-crashed (and restarts) whenever a batch holds a call whose
first argument is 13.0 (402a000000000000), the marked call; HEAVY costs 20 ms per call; a start also fails while the file named by
TOUCHSTONE_TEST_FAIL_FLAG exists and holds this worker's name (or nothing).
EXCELERROR reports an Excel error and makes the next batch hit a fingerprint check before evaluation.
"""
import json
import os
import struct
import sys
import time


def read_exact(n):
    data = b""
    while len(data) < n:
        chunk = sys.stdin.buffer.read(n - len(data))
        if not chunk:
            raise EOFError
        data += chunk
    return data


def send(kind, obj):
    body = json.dumps(obj, ensure_ascii=True, separators=(",", ":")).encode("ascii")
    sys.stdout.buffer.write(struct.pack(">I", len(body) + 1) + kind.encode() + body)
    sys.stdout.buffer.flush()


def hello(restart=False):
    send("H", {"name": NAME, "pid": os.getpid(), "start_ticks": 1, "launch": 1, "env_id": "fake00000001",
               "env": {"engine": "fake", "version": "0", "build": "0"}, "restart": restart, "worker_pid": os.getpid()})


NAME = sys.argv[sys.argv.index("--name") + 1]
if os.environ.get("TOUCHSTONE_TEST_FAIL_START") == NAME:
    send("X", {"fatal": "excel-start-failed", "detail": "fake start refused"})
    sys.exit(3)
flag = os.environ.get("TOUCHSTONE_TEST_FAIL_FLAG")
if flag and os.path.exists(flag) and open(flag).read().strip() in ("", NAME):
    send("X", {"fatal": "excel-start-failed", "detail": "fake start refused (flag)"})
    sys.exit(3)
hello()
crashed = False
drifted = False
while True:
    try:
        (size,) = struct.unpack(">I", read_exact(4))
        body = read_exact(size)
    except EOFError:
        break
    kind, payload = chr(body[0]), json.loads(body[1:])
    if kind == "Q":
        break
    if kind == "R":
        drifted = False
        hello(True)
        continue
    if kind != "B":
        continue
    groups = [g for g in payload["groups"] if not g.get("p")]          # primer calls are evaluated, not answered
    if payload.get("serial") and any("o" not in g and "i" not in g for g in groups):
        send("R", {"id": payload["id"], "error": "excel-error", "detail": "serial group has no request positions"})
        continue
    if drifted:
        send("R", {"id": payload["id"], "error": "fingerprint-drift-before", "detail": "fake settings changed"})
        drifted = False
        hello(True)
        continue
    if any(g["f"] == "EXCELERROR" for g in groups):
        send("R", {"id": payload["id"], "env_id": "fake00000001", "worker": NAME, "batch": payload["id"],
                   "error": "excel-error", "detail": "fake Excel error"})
        drifted = True
        continue
    if any(g["f"] == "DIE" for g in groups):
        os._exit(1)
    marked = any(g["f"] == "CRASHON" and any(g["v"][i * max(1, len(g["v"]) // g["n"])] == "402a000000000000" for i in range(g["n"]))
                 for g in groups)
    if any(g["f"] == "CRASH" for g in groups) and not crashed or any(g["f"] == "CRASHALWAYS" for g in groups) or marked:
        crashed = True
        send("R", {"id": payload["id"], "env_id": "fake00000001", "worker": NAME, "batch": 1, "error": "excel-crashed",
                   "detail": "fake crash", "deadline": False})
        time.sleep(0.1)
        hello(True)
        continue
    if any(g["f"] == "SLEEP" for g in groups):
        time.sleep(0.3)
    heavy = sum(g["n"] for g in groups if g["f"] == "HEAVY")
    if heavy:
        time.sleep(0.02 * heavy)
    out = []
    for g in groups:
        tokens = g["k"].split(",") if g["k"] else []
        width = sum(int(t[1:].split("x")[0]) * int(t[1:].split("x")[1]) if t[0] in "arm" else 1 for t in tokens)
        if g["f"] == "ID":
            vals = [g["v"][i * width] for i in range(g["n"])]
            out.append({"ty": "n" * g["n"], "v": vals})
        else:
            out.append({"ty": "n" * g["n"], "v": ["3ff0000000000000"] * g["n"]})
        if payload.get("echo"):
            letters, echoed = [], []
            for i in range(g["n"]):
                at = i * width
                for token in tokens:
                    n = int(token[1:].split("x")[0]) * int(token[1:].split("x")[1]) if token[0] in "arm" else 1
                    for value in g["v"][at:at + n]:
                        if token[0] in "rm":
                            kind = value["type"]
                            letters.append({"number":"n", "text":"t", "boolean":"b", "blank":"z", "error":"e"}[kind])
                            echoed.append(value.get("bits", value.get("value", value.get("code"))))
                        else:
                            letters.append({"n":"n", "a":"n", "s":"t", "S":"t", "b":"b", "B":"b", "z":"z", "e":"e", "f":"t"}[token[0]])
                            echoed.append(value)
                    at += n
            out[-1]["ity"] = "".join(letters)
            out[-1]["iv"] = echoed
    send("R", {"id": payload["id"], "env_id": "fake00000001", "worker": NAME, "batch": payload["id"], "groups": out,
               "timing": {"write_ms": 1.0, "calc_ms": 1.0 + 20.0 * heavy, "read_ms": 1.0, "convert_ms": 1.0, "total_ms": 4.0 + 20.0 * heavy}})
