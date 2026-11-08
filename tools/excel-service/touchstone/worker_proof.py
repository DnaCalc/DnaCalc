r"""WP1 proof: one C# worker against the WP0 sample, 100k ERF.PRECISE calls, and a deadline kill.

Run under tools/excel-session/on-private-desktop.ps1 (it starts Excel):
  py -3 worker_proof.py --state C:\Work\DnaCalc\codex-runs\excel-service-v2 --out proof-wp1.json
"""
import argparse
import json
import random
import statistics
import struct
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import engine  # noqa: E402


def excel_pids():
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq EXCEL.EXE", "/FO", "CSV", "/NH"], capture_output=True, text=True).stdout
    return sorted(int(line.split('","')[1]) for line in out.splitlines() if line.startswith('"EXCEL'))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--state", type=Path, required=True)
    parser.add_argument("--sample", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--calls", type=int, default=100000)
    options = parser.parse_args()
    sample = json.loads((options.sample or options.state / "wp0-sample.json").read_text())
    report = {"excel_before": excel_pids()}
    worker = engine.WorkerProcess(options.state / "excel", "p1", mt=4)
    t0 = time.monotonic()
    hello = worker.start()
    report["start_seconds"] = time.monotonic() - t0
    report["hello"] = hello
    report["excel_with_worker"] = excel_pids()
    try:
        # 1. WP0 sample, bit-identical
        t0 = time.monotonic()
        result = worker.run({"id": 1, "groups": sample["groups"], "deadline_ms": 30000}, 60)
        report["sample_wall_seconds"] = time.monotonic() - t0
        report["sample_error"] = [result.get("error"), result.get("detail")]
        matched, mismatched, unread = 0, [], 0
        for group, expected, got in zip(sample["groups"], sample["expected"], result.get("groups", [])):
            for i, (el, ev, gl, gv) in enumerate(zip(expected["ty"], expected["v"], got["ty"], got["v"])):
                if gl == "u":
                    unread += 1
                if (el, ev) == (gl, gv):
                    matched += 1
                else:
                    mismatched.append({"f": group["f"], "i": i, "expected": [el, ev], "got": [gl, gv]})
        report["sample"] = {"calls": sample["calls"], "matched": matched, "unread": unread,
                            "mismatched": len(mismatched), "first_mismatches": mismatched[:8],
                            "timing": result.get("timing")}
        # 2. 100k ERF.PRECISE
        rnd = random.Random(7)
        values = [format(struct.unpack(">Q", struct.pack(">d", rnd.uniform(0.0, 5.0)))[0], "016x") for _ in range(options.calls)]
        group = {"f": "ERF.PRECISE", "k": "n", "n": len(values), "v": values}
        times = []
        for repeat in range(3):
            t0 = time.monotonic()
            result = worker.run({"id": 10 + repeat, "groups": [group], "deadline_ms": 30000}, 60)
            wall = time.monotonic() - t0
            times.append({"wall": wall, "worker_total_ms": result.get("timing", {}).get("total_ms"), "timing": result.get("timing"), "detail": result.get("detail"),
                          "error": result.get("error")})
        report["erf_100k"] = times
        got = result["groups"][0] if "groups" in result else {"ty": ""}
        report["erf_100k_all_numbers"] = set(got["ty"]) == {"n"}
        # 3. typed arguments: text, boolean, error, blank, array, literal
        mix = [
            {"f": "LEN", "k": "s", "n": 2, "v": ["abc", "h\u00e9llo"]},
            {"f": "AND", "k": "b,b", "n": 1, "v": [True, False]},
            {"f": "ISERROR", "k": "e", "n": 2, "v": ["#N/A", "#DIV/0!"]},
            {"f": "ISBLANK", "k": "z", "n": 1, "v": [None]},
            {"f": "LEN", "k": "S", "n": 1, "v": ["a\"b"]},
            {"f": "SUM", "k": "a2x2", "n": 1, "v": ["3ff0000000000000", "4000000000000000", "4008000000000000", "4010000000000000"]},
            {"f": "LINEST", "k": "a3x1,a3x1,b,b", "x": [1, 1], "n": 1, "v": [
                "4000000000000000", "4010000000000000", "4018000000000000",
                "3ff0000000000000", "4000000000000000", "4008000000000000", False, False]},
            {"f": "SQRT", "k": "n", "n": 1, "v": ["bff0000000000000"]},
            {"f": "ABS", "k": "n", "n": 1, "v": ["0000000000000001"]},
        ]
        result = worker.run({"id": 20, "groups": mix, "deadline_ms": 30000, "echo": True}, 60)
        report["typed"] = result.get("groups", result)
        # 4. a missed deadline kills only this worker's Excel
        others = [p for p in excel_pids() if p != hello["pid"]]
        heavy = {"f": "ERF.PRECISE", "k": "n", "n": len(values), "v": values}
        t0 = time.monotonic()
        result = worker.run({"id": 30, "groups": [heavy], "deadline_ms": 5}, 60)
        report["deadline"] = {"error": result.get("error"), "detail": result.get("detail"), "deadline": result.get("deadline"),
                              "wall_seconds": time.monotonic() - t0}
        if result.get("error") == "excel-crashed":
            new = worker.await_restart()
            report["deadline"]["restart_pid"] = new["pid"]
            report["deadline"]["restart_differs"] = new["pid"] != hello["pid"]
            result = worker.run({"id": 31, "groups": [{"f": "ABS", "k": "n", "n": 1, "v": ["bff0000000000000"]}], "deadline_ms": 30000}, 60)
            report["deadline"]["after_restart"] = result.get("groups")
        report["excel_others_before_kill"] = others
        report["excel_others_after_kill"] = [p for p in excel_pids() if p in others]
    finally:
        worker.stop()
    report["excel_after"] = excel_pids()
    options.out.write_text(json.dumps(report, indent=1))
    summary = {k: report[k] for k in ("start_seconds", "sample_wall_seconds") if k in report}
    summary["sample"] = {k: v for k, v in report["sample"].items() if k != "first_mismatches"}
    summary["erf_100k_worker_ms"] = [t["worker_total_ms"] for t in report["erf_100k"]]
    summary["deadline"] = report.get("deadline")
    print(json.dumps(summary, indent=1))


if __name__ == "__main__":
    main()
