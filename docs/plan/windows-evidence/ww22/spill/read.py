import json
from pathlib import Path
import sys
import time
sys.path.insert(0, str(Path.cwd() / "tools/excel-service"))
from client import Client, number, number_array

root = Path(__file__).parent
client = Client(timeout=25)
def typed(value):
    return number_array(value) if isinstance(value, list) else number(value)

def ask(out, name, label, args):
    start = time.monotonic()
    try:
        reply = client.ask(name, [[typed(arg) for arg in row] for row in args], shape="array", echo_inputs=True)
    except Exception as exc:
        reply = {"error": str(exc), "type": type(exc).__name__, "response": getattr(exc, "response", None)}
    out.write(json.dumps(dict(name=name, label=label, args=args, reply=reply, seconds=round(time.monotonic()-start, 3))) + "\n")
    out.flush()

with (root / "reads.jsonl").open("w", encoding="utf-8") as out:
    for cohort in ("450-shapes", "451", "441-sweep-reps"):
        inputs = json.loads((root / f"{cohort}-input.json").read_text())
        name, args = next(iter(inputs.items()))
        ask(out, name, f"{cohort}-batch", args)
        if len(args) > 1:
            for i, row in enumerate(args):
                ask(out, name, f"{cohort}-isolated-{i}", [row])
