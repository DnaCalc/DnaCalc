"""Excel answer service measurements.

  bench.py sample --out FILE            build the WP0 sample (already-answered calls from the v1 log)
  bench.py log-stats [--limit N]        today's v1 numbers from the retained request folders
  bench.py v1 --sample FILE [--functions N]   replay part of the sample through the v1 queue
  bench.py --host X --calls N --function F    time N generated calls through a touchstone endpoint
                                             (X: local, or an ssh host alias)
"""
import argparse
import json
import random
import statistics
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE / "touchstone"))

import protocol  # noqa: E402

V1_STATE = Path(r"C:\Work\DnaCalc\codex-runs\excel-service")
V2_STATE = Path(r"C:\Work\DnaCalc\codex-runs\excel-service-v2")
SKIP = {"WEEKNUM"}


def iter_requests(state, seed=1):
    paths = [p for origin in sorted((state / "requests").iterdir()) for p in origin.iterdir()]
    random.Random(seed).shuffle(paths)
    for path in paths:
        question, answer = path / "question.json", path / "answer.json"
        if not (question.exists() and answer.exists()):
            continue
        try:
            q = json.loads(question.read_text(encoding="utf-8"))
            a = json.loads(answer.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        yield path, q, a


def build_sample(state, per_function=33, target_functions=150, target_calls=5000):
    by_function = {}
    for path, q, a in iter_requests(state):
        if not a.get("ok") or q["function"] in SKIP or len(a["rows"]) != len(q["rows"]):
            continue
        have = by_function.setdefault(q["function"], {"rows": [], "answers": [], "x": q.get("result_index")})
        if len(have["rows"]) >= per_function or have["x"] != q.get("result_index"):
            continue
        take = min(per_function - len(have["rows"]), len(q["rows"]), 12)
        start = random.Random(path.name).randrange(0, max(1, len(q["rows"]) - take + 1))
        # The inputs Excel read back are the inputs it calculated on: v1 turned a lone UTF-16 surrogate
        # into U+FFFD on the way in, so the recorded answer belongs to the read-back input.
        for row, inputs in zip(q["rows"][start:start + take], a["inputs"][start:start + take]):
            have["rows"].append([{**got, **({"mode": want["mode"]} if "mode" in want else {})}
                                 for want, got in zip(row, inputs)])
        have["answers"].extend(a["rows"][start:start + take])
        if sum(len(v["rows"]) for v in by_function.values()) >= target_calls and len(by_function) >= target_functions:
            break
    groups, expected = [], []
    for function in sorted(by_function):
        have = by_function[function]
        if not have["rows"]:
            continue
        for group, rows_positions in _split(function, have):
            groups.append(group)
    # expected answers aligned with each group's call order
    for group in groups:
        expected.append(group.pop("_expected"))
    return {"calls": sum(g["n"] for g in groups), "functions": len({g["f"] for g in groups}),
            "groups": groups, "expected": expected, "source": str(state)}


def _split(function, have):
    for group in protocol.rows_to_groups(function, have["rows"], have["x"]):
        positions = group.get("i") or list(range(group["n"]))
        group["_expected"] = {"ty": "", "v": []}
        for p in positions:
            letter, value = protocol.typed_to_answer(have["answers"][p])
            group["_expected"]["ty"] += letter
            group["_expected"]["v"].append(value)
        # keep positions local to this group so the sample can be replayed by itself
        group.pop("i", None)
        yield group, positions


def log_stats(state, limit):
    seconds, calls_per, rows_seconds = [], [], {1: []}
    count = 0
    for path, q, a in iter_requests(state):
        if "service_seconds" not in a:
            continue
        count += 1
        seconds.append(a["service_seconds"])
        calls_per.append(len(q["rows"]))
        if len(q["rows"]) == 1:
            rows_seconds[1].append(a["service_seconds"])
        if count >= limit:
            break
    seconds.sort()
    total_calls = sum(calls_per)
    result = {"requests": count, "calls": total_calls,
              "service_seconds_min": seconds[0], "service_seconds_median": statistics.median(seconds),
              "service_seconds_p90": seconds[int(len(seconds) * .9)],
              "one_row_median": statistics.median(rows_seconds[1]) if rows_seconds[1] else None,
              "ms_per_row": 1000 * sum(seconds) / total_calls}
    return result


def run_v1(sample_path, functions):
    from client import Client, number
    sample = json.loads(Path(sample_path).read_text())
    client = Client(timeout=300)
    timings, matched, total = [], 0, 0
    for group, expected in list(zip(sample["groups"], sample["expected"]))[:functions]:
        rows = protocol.group_to_rows(group)
        started = time.monotonic()
        reply = client.ask(group["f"], rows, result_index=group.get("x"))
        timings.append((time.monotonic() - started, len(rows)))
        for typed, letter, value in zip(reply["rows"], expected["ty"], expected["v"]):
            total += 1
            got = protocol.typed_to_answer(typed)
            matched += got == (letter, value)
    secs = sorted(t for t, _ in timings)
    return {"requests": len(timings), "calls": total, "matched": matched,
            "wall_seconds_min": secs[0], "wall_seconds_median": statistics.median(secs),
            "wall_seconds_max": secs[-1]}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", nargs="?", choices=["sample", "log-stats", "v1", "run"], default="run")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--sample", type=Path, default=V2_STATE / "wp0-sample.json")
    parser.add_argument("--state", type=Path, default=V1_STATE)
    parser.add_argument("--limit", type=int, default=1500)
    parser.add_argument("--functions", type=int, default=20)
    parser.add_argument("--host", default="local")
    parser.add_argument("--calls", type=int, default=10000)
    parser.add_argument("--function", default="ERF.PRECISE")
    parser.add_argument("--repeat", type=int, default=5)
    options = parser.parse_args()
    if options.command == "sample":
        sample = build_sample(options.state)
        options.out = options.out or options.sample
        options.out.parent.mkdir(parents=True, exist_ok=True)
        options.out.write_text(json.dumps(sample, separators=(",", ":")), encoding="utf-8")
        print(json.dumps({k: sample[k] for k in ("calls", "functions", "source")}))
    elif options.command == "log-stats":
        print(json.dumps(log_stats(options.state, options.limit), indent=1))
    elif options.command == "v1":
        print(json.dumps(run_v1(options.sample, options.functions), indent=1))
    else:
        from touchstone.bench_run import run
        print(json.dumps(run(options), indent=1))


if __name__ == "__main__":
    main()
