r"""WP2 proof: three bulk clients and one interactive client against a running broker.

    py -3 load_test.py --state C:\Work\DnaCalc\codex-runs\excel-service-v2 [--mix sample|numeric] [--out FILE]

Round 2 proof (excel-v2-2): `--mix sample --calls 333334 --bulk-clients 3` is a 1M-call mixed replay of the
188-function baseline sample while an interactive prober asks one call every 100 ms with a 15 s timeout;
the report counts probes over 1 s and probe timeouts.
"""
import argparse
import json
from pathlib import Path
import statistics
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from touchstone import client  # noqa: E402
import protocol  # noqa: E402


def tile(sample, calls):
    """Repeat the sample's groups until about `calls` calls; returns groups (with positions) and expected."""
    groups, expected = [], []
    total = 0
    if sample["expected"] and sample["expected"][0] is None:
        groups = protocol.with_positions(sample["groups"])
        return groups, [{"ty": "", "v": []} for _ in groups], sum(g["n"] for g in groups)
    while total < calls:
        for g, e in zip(sample["groups"], sample["expected"]):
            if total >= calls:
                break
            groups.append(g)
            expected.append(e)
            total += g["n"]
    return protocol.with_positions(groups), expected, total


NUMERIC_ONLY = {"n"}


def numeric_mix(sample, functions=20):
    chosen = [(g, e) for g, e in zip(sample["groups"], sample["expected"]) if g["k"] and set(g["k"].split(",")) <= NUMERIC_ONLY and "x" not in g]
    seen, picked = set(), []
    for g, e in chosen:
        if g["f"] not in seen:
            seen.add(g["f"])
            picked.append((g, e))
        if len(picked) >= functions:
            break
    return {"groups": [g for g, _ in picked], "expected": [e for _, e in picked]}


CHEAP = ["ABS", "SQRT", "EXP", "LN", "SIN", "COS", "TAN", "ATAN", "ERF.PRECISE", "ERFC.PRECISE", "GAMMALN", "LOG10",
         "SINH", "COSH", "TANH", "ASIN", "ACOS", "DEGREES", "RADIANS", "INT"]


def cheap_mix(calls_per_client, seed=11):
    """Twenty elementary functions, random inputs; the expected answers are checked for type only."""
    import random
    import struct
    rnd = random.Random(seed)
    groups = []
    per = max(1, calls_per_client // len(CHEAP))
    for f in CHEAP:
        values = [format(struct.unpack(">Q", struct.pack(">d", rnd.uniform(0.01, 0.99)))[0], "016x") for _ in range(per)]
        groups.append({"f": f, "k": "n", "n": per, "v": values})
    return {"groups": groups, "expected": [None] * len(groups)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--state", type=Path, required=True)
    parser.add_argument("--sample", type=Path)
    parser.add_argument("--mix", choices=["sample", "numeric", "cheap"], default="numeric")
    parser.add_argument("--calls", type=int, default=50000)
    parser.add_argument("--bulk-clients", type=int, default=3)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--probe-timeout", type=float, default=15.0, help="seconds before a one-call probe counts as timed out")
    parser.add_argument("--probe-functions", nargs="*", default=["ERF.PRECISE", "SQRT", "GAMMALN", "ABS"])
    options = parser.parse_args()
    sample = json.loads((options.sample or options.state / "wp0-sample.json").read_text())
    if options.mix == "numeric":
        sample = numeric_mix(sample)
    elif options.mix == "cheap":
        sample = cheap_mix(options.calls)
    endpoint = options.state / "endpoint.json"
    bulk = [client.Connection.open(endpoint=endpoint, socket_path=options.state / "none.sock", name=f"bulk{i}") for i in range(options.bulk_clients)]
    inter = client.Connection.open(endpoint=endpoint, socket_path=options.state / "none.sock", name="inter")
    work = [tile(sample, options.calls) for _ in bulk]
    stop = threading.Event()
    latencies, errors = [], []
    probe_value = "3ff0000000000000"

    timeouts = []

    def interactive():
        n = 0
        while not stop.is_set():
            t0 = time.monotonic()
            function = options.probe_functions[n % len(options.probe_functions)]
            n += 1
            try:
                p = inter.submit_groups([{"f": function, "k": "n", "n": 1, "v": [probe_value]}])
                p.raw(options.probe_timeout)
                latencies.append(time.monotonic() - t0)
            except TimeoutError as error:
                timeouts.append(function)
            except Exception as error:
                if "timed out" in str(error) or "timeout" in str(error).lower():
                    timeouts.append(function)
                errors.append(str(error))
            stop.wait(max(0, 0.1 - (time.monotonic() - t0)))

    thread = threading.Thread(target=interactive)
    t0 = time.monotonic()
    pendings = [c.submit_groups(groups, priority="bulk") for c, (groups, _, _) in zip(bulk, work)]
    time.sleep(0.3)
    thread.start()
    results = []
    for pending, (groups, expected, calls) in zip(pendings, work):
        letters, values = pending.raw(600)
        results.append((letters, values, pending.seconds))
    wall = time.monotonic() - t0
    stop.set()
    thread.join()
    matched = mismatched = 0
    by_function = {}
    typed_ok = 0
    for (letters, values, _), (groups, expected, calls) in zip(results, work):
        typed_ok += sum(1 for l in letters if l in ("n", "e"))
        for g, e in zip(groups, expected):
            for p, el, ev in zip(range(g["o"], g["o"] + g["n"]), e["ty"], e["v"]):
                if (letters[p], values[p]) == (el, ev):
                    matched += 1
                else:
                    mismatched += 1
                    by_function.setdefault(g["f"], []).append([el, ev, letters[p], values[p]])
    total_calls = sum(c for _, _, c in work)
    latencies.sort()
    report = {"mix": options.mix, "bulk_clients": len(bulk), "calls": total_calls, "wall_seconds": round(wall, 3),
              "calls_per_second": round(total_calls / wall), "matched": matched, "mismatched": mismatched, "number_or_error_answers": typed_ok,
              "interactive_count": len(latencies), "interactive_errors": errors[:3], "interactive_error_count": len(errors),
              "interactive_timeouts": len(timeouts), "interactive_over_1s": sum(1 for x in latencies if x > 1.0),
              "interactive_ms_p50": round(1000 * latencies[len(latencies) // 2], 1) if latencies else None,
              "interactive_ms_p99": round(1000 * latencies[min(len(latencies) - 1, int(len(latencies) * .99))], 1) if latencies else None,
              "interactive_ms_max": round(1000 * latencies[-1], 1) if latencies else None,
              "mismatch_by_function": {f: {"count": len(v), "example": v[0]} for f, v in by_function.items()},
              "bulk_seconds": [round(s, 2) for _, _, s in results]}
    print(json.dumps(report, indent=1))
    if options.out:
        options.out.write_text(json.dumps(report, indent=1))


if __name__ == "__main__":
    main()
