r"""WP3 proofs from the Linux hosts (run on Windows; the measurements run on the host through its relay).

  py -3 proof_wp3.py compare|v1client|killssh --host dna-firehorse --state C:\Work\DnaCalc\codex-runs\excel-service-v2
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import threading
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))

import peers  # noqa: E402
import protocol  # noqa: E402


def remote(host, modules, source, extra=None, timeout=600):
    command, first = peers.bundle_program(modules, source, extra)
    done = subprocess.run(["ssh", "-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", host, command],
                          input=first, capture_output=True, timeout=timeout)
    if done.returncode:
        raise RuntimeError(done.stderr.decode("utf-8", "replace")[-2000:])
    return json.loads(done.stdout.decode().strip().splitlines()[-1])


def sample_500(state):
    sample = json.loads((state / "wp0-sample.json").read_text())
    groups, expected, total = [], [], 0
    for g, e in zip(sample["groups"], sample["expected"]):
        if g["f"] in ("TEXT",):         # the one case whose old recorded answer v1 no longer reproduces
            continue
        if g["k"].startswith("a"):
            continue
        groups.append(g)
        expected.append(e)
        total += g["n"]
        if total >= 500:
            break
    return groups, expected, total


COMPARE_RUNNER = r'''
import json, time
conn = sys.modules["client"].Connection.open(name="compare")
groups = json.loads(GROUPS)
t0 = time.perf_counter()
p = conn.submit_groups(groups)
letters, values = p.raw(300)
print(json.dumps({"letters": letters, "values": values, "seconds": time.perf_counter() - t0}))
'''

V1CLIENT_RUNNER = r'''
import json, time
V1 = sys.modules["v1client"]
c = V1.Client(queue="~/excel-queue-v2", timeout=60)
times = []
answers = []
for i in range(30):
    t0 = time.perf_counter()
    r = c.ask("ERF.PRECISE", [[V1.number("3e47bc15e9263272")]])
    times.append(time.perf_counter() - t0)
    answers.append(r["rows"][0]["bits"])
times.sort()
print(json.dumps({"p50_ms": round(1000 * times[len(times) // 2], 1), "p99_ms": round(1000 * times[-1], 1),
                  "answers": sorted(set(answers)), "build": r["excel_build"], "machine": r["machine"],
                  "inputs": r["inputs"][0]}))
'''

KILL_RUNNER = r'''
import json, struct, random, time
conn = sys.modules["client"].Connection.open(name="killssh")
rnd = random.Random(3)
pend = []
for r in range(ROUNDS):
    vals = [format(struct.unpack(">Q", struct.pack(">d", rnd.uniform(0.01, 0.99)))[0], "016x") for _ in range(N)]
    pend.append(conn.submit_groups([{"f": "ERF.PRECISE", "k": "n", "n": N, "v": vals}], priority="bulk"))
t0 = time.perf_counter()
done = [p.raw(180) for p in pend]
print(json.dumps({"seconds": round(time.perf_counter() - t0, 2), "answered": [len(d[1]) for d in done],
                  "all_numbers": all(set(d[0]) == {"n"} for d in done), "reconnects": conn.reconnects}))
'''


def compare(options):
    """Same 500 calls: v1 live service (Windows file queue) vs touchstone from the Linux host."""
    groups, expected, total = sample_500(options.state)
    from client import Client
    v1_answers = []
    t0 = time.monotonic()
    client = Client(timeout=600)
    for g in groups:
        reply = client.ask(g["f"], protocol.group_to_rows(g), result_index=g.get("x"))
        v1_answers.append([protocol.typed_to_answer(r) for r in reply["rows"]])
    v1_seconds = time.monotonic() - t0
    source = f"GROUPS = {json.dumps(protocol.with_positions(groups))!r}\n" + COMPARE_RUNNER
    out = remote(options.host, ("protocol", "v1", "client"), source)
    flat_v1 = [a for answers in v1_answers for a in answers]
    v2 = list(zip(out["letters"], out["values"]))
    same = sum(1 for a, b in zip(flat_v1, v2) if list(a) == list(b))
    rec = [(l, v) for e in expected for l, v in zip(e["ty"], e["v"])]
    same_recorded = sum(1 for a, b in zip(rec, v2) if list(a) == list(b))
    report = {"host": options.host, "calls": total, "functions": len(groups), "v1_equal": same, "v1_total": len(flat_v1),
              "recorded_equal": same_recorded, "v1_seconds": round(v1_seconds, 2), "v2_seconds": round(out["seconds"], 3),
              "differences": [[i, list(a), list(b)] for i, (a, b) in enumerate(zip(flat_v1, v2)) if list(a) != list(b)][:5]}
    print(json.dumps(report, indent=1))
    return report


def v1client(options):
    """The v1 Client against the relay's file queue, timed from the host."""
    out = remote(options.host, ("protocol", "v1", "v1client"), V1CLIENT_RUNNER, {"v1client": HERE.parent / "client.py"})
    print(json.dumps({"host": options.host, **out}, indent=1))
    return out


def killssh(options):
    """Cut the broker's ssh session mid-stream: every request still gets all its answers, once."""
    state = options.state
    status = json.loads((state / "status.json").read_text())
    before_calls = status["counters"]["calls"]
    n, rounds = 40000, 25
    source = f"N = {n}\nROUNDS = {rounds}\n" + KILL_RUNNER
    result = {}

    def work():
        result.update(remote(options.host, ("protocol", "v1", "client"), source))

    thread = threading.Thread(target=work)
    thread.start()
    time.sleep(4.0)
    ps = subprocess.run(["powershell", "-NoProfile", "-Command",
                         f"Get-CimInstance Win32_Process -Filter \"Name='ssh.exe' AND ParentProcessId={status['pid']}\" | "
                         "Where-Object { $_.CommandLine -like '*" + options.host + "*closefd=False*' } | "
                         "Select-Object -ExpandProperty ProcessId"], capture_output=True, text=True)
    killed = []
    for pid in (int(x) for x in ps.stdout.split()):
        subprocess.run(["taskkill", "/F", "/PID", str(pid)], capture_output=True)   # the broker's own ssh child for this host
        killed.append(pid)
    thread.join(300)
    time.sleep(1)
    after = json.loads((state / "status.json").read_text())
    evaluated = after["counters"]["calls"] - before_calls
    report = {"host": options.host, "ssh_killed": killed, "client": result, "calls_requested": n * rounds,
              "calls_evaluated_by_broker": evaluated, "duplicate_evaluation": evaluated != n * rounds}
    print(json.dumps(report, indent=1))
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["compare", "v1client", "killssh"])
    parser.add_argument("--host", default="dna-firehorse")
    parser.add_argument("--state", type=Path, default=Path(r"C:\Work\DnaCalc\codex-runs\excel-service-v2"))
    options = parser.parse_args()
    {"compare": compare, "v1client": v1client, "killssh": killssh}[options.command](options)
