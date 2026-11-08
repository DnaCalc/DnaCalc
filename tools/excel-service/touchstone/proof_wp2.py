r"""WP2 proof against a running broker: mixed load, a killed worker, a worker that fails to start.

  py -3 proof_wp2.py load|kill --state C:\Work\DnaCalc\codex-runs\excel-service-v2
"""
import argparse
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from touchstone import client  # noqa: E402
import engine  # noqa: E402


def status(state):
    for _ in range(50):
        try:
            data = json.loads((state / "status.json").read_text())
            if "workers" in data:
                return data
        except (OSError, ValueError):
            pass
        time.sleep(0.1)
    raise RuntimeError("no status")


def kill(state):
    before = status(state)
    victims = {w["name"]: w for w in before["workers"]}
    conn = client.Connection.open(endpoint=state / "endpoint.json", socket_path=state / "none.sock")
    # a big request keeps every worker busy
    sample = json.loads((state / "wp0-sample.json").read_text())
    from load_test import tile  # noqa
    groups, expected, calls = tile(sample, 40000)
    pending = conn.submit_groups(groups, priority="bulk")
    time.sleep(0.4)
    busy = [w for w in status(state)["workers"] if w["state"] == "busy"]
    target = busy[0] if busy else before["workers"][0]
    # the victim's hello carries the start ticks; the broker status has the pid, ticks come from the OS
    identity = engine.process_identity(target["pid"])
    outcome = engine.kill_owned_excel(target["pid"], identity[1]) if identity else "gone"
    t0 = time.monotonic()
    error = None
    try:
        pending.raw(120)
    except Exception as exc:
        error = str(exc)
    after = None
    for _ in range(100):
        after = status(state)
        w = {x["name"]: x for x in after["workers"]}[target["name"]]
        if w["state"] == "idle" and w["pid"] != target["pid"]:
            break
        time.sleep(0.5)
    others_same = all(w["pid"] == victims[w["name"]]["pid"] for w in after["workers"] if w["name"] != target["name"])
    # the pool still answers
    p = conn.submit_groups([{"f": "ABS", "k": "n", "n": 2, "v": ["bff0000000000000", "3ff0000000000000"]}])
    letters, values = p.raw(30)
    report = {"killed_worker": target["name"], "killed_pid": target["pid"], "kill_outcome": outcome,
              "request_error": error, "worker_after": {x["name"]: [x["state"], x["pid"]] for x in after["workers"]},
              "other_workers_unchanged": others_same, "next_answer": list(zip(letters, values)),
              "seconds_to_request_end": round(time.monotonic() - t0, 2), "crashes": [w["crashes"] for w in after["workers"]]}
    print(json.dumps(report, indent=1))
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["kill"])
    parser.add_argument("--state", type=Path, required=True)
    options = parser.parse_args()
    kill(options.state)
