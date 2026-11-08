"""bench.py --host X --calls N --function F: time generated calls through a touchstone endpoint.

host "local" uses the Windows broker's TCP endpoint; any other host is an ssh alias, and the measurement runs
on that host through its relay socket (the code travels in the ssh command; nothing is installed).
"""
import json
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent

RUNNER = r'''
import json, random, statistics, struct, time
args = json.loads(ARGS)
conn = sys.modules["client"].Connection.open(endpoint=args.get("endpoint"), name="bench")
rnd = random.Random(5)
def hexes(n):
    return [format(struct.unpack(">Q", struct.pack(">d", rnd.uniform(0.01, 0.99)))[0], "016x") for _ in range(n)]
out = {"function": args["function"], "calls": args["calls"], "host": __import__("socket").gethostname()}
# single questions
single = []
for _ in range(args["singles"]):
    t0 = time.perf_counter()
    conn.submit_groups([{"f": args["function"], "k": "n", "n": 1, "v": hexes(1)}]).raw(60)
    single.append(time.perf_counter() - t0)
single.sort()
out["single_ms"] = {"p50": round(1000 * single[len(single) // 2], 1), "p99": round(1000 * single[min(len(single) - 1, int(len(single) * .99))], 1),
                    "max": round(1000 * single[-1], 1), "n": len(single)}
# one big request, repeated
big = []
for _ in range(args["repeat"]):
    g = {"f": args["function"], "k": "n", "n": args["calls"], "v": hexes(args["calls"])}
    t0 = time.perf_counter()
    letters, values = conn.submit_groups([g], priority="bulk").raw(300)
    big.append(time.perf_counter() - t0)
    assert len(values) == args["calls"] and set(letters) <= {"n", "e"}
out["bulk_seconds"] = [round(x, 3) for x in big]
out["bulk_seconds_median"] = round(statistics.median(big), 3)
print(json.dumps(out))
'''


def run(options):
    args = {"function": options.function, "calls": options.calls, "singles": 40, "repeat": options.repeat}
    if options.host == "local":
        sys.path.insert(0, str(HERE.parent))
        args["endpoint"] = str(Path(options.state_v2 if hasattr(options, "state_v2") else r"C:\Work\DnaCalc\codex-runs\excel-service-v2") / "endpoint.json")
        from touchstone import client, protocol, v1
        sys.modules["client"] = client
        scope = {"sys": sys, "ARGS": json.dumps(args)}
        import io
        from contextlib import redirect_stdout
        buffer = io.StringIO()
        with redirect_stdout(buffer):
            exec(compile(RUNNER, "runner", "exec"), scope)
        return json.loads(buffer.getvalue())
    from peers import bundle_program
    command, first = bundle_program(("protocol", "v1", "client"), f"ARGS = {json.dumps(args)!r}\n" + RUNNER)
    done = subprocess.run(["ssh", "-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", options.host, command],
                          input=first, capture_output=True, timeout=900)
    done.stdout, done.stderr = done.stdout.decode("utf-8", "replace"), done.stderr.decode("utf-8", "replace")
    if done.returncode:
        raise RuntimeError(done.stderr[-2000:])
    return json.loads(done.stdout.strip().splitlines()[-1])
