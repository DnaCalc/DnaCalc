import json
from pathlib import Path
import sys
import time
sys.path.insert(0, str(Path.cwd() / "tools/excel-service"))
from client import Client, number, boolean

args = [number("3fb999999999999a"), number("7fdd65f0937da3fa"), number("3ff0000000000000"), boolean(True)]
out = Path(__file__).with_name("gamma.jsonl")
client = Client(timeout=40)
with out.open("w", encoding="utf-8") as log:
    for function in ("GAMMA.DIST", "GAMMADIST"):
        start = time.monotonic()
        try:
            reply = client.ask(function, [args], echo_inputs=True)
        except Exception as exc:
            reply = {"error": str(exc), "type": type(exc).__name__, "response": getattr(exc, "response", None)}
        row = dict(function=function, args=args, reply=reply, seconds=round(time.monotonic() - start, 3))
        log.write(json.dumps(row) + "\n")
        log.flush()
