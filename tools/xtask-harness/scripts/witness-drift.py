#!/usr/bin/env python3
"""Compare witness export rows to the Excel service, preserving changed-input and failure evidence."""
import argparse
import collections
import concurrent.futures
import gzip
import json
import pathlib
import re
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools/excel-service"))
from client import Client, boolean, number, number_array, text  # noqa: E402

BITS = re.compile(r"^0x[0-9a-fA-F]{16}$")
ERRORS = {"#NUM!": "Num", "#DIV/0!": "Div0", "#VALUE!": "Value", "#N/A": "NA",
          "#REF!": "Ref", "#NAME?": "Name", "#NULL!": "Null", "#SPILL!": "Spill",
          "#CALC!": "Calc", "#GETTING_DATA": "GettingData"}


def arg(value):
    if isinstance(value, bool):
        return boolean(value)
    if isinstance(value, str):
        return number(value[2:]) if BITS.fullmatch(value) else text(value)
    if isinstance(value, list):
        rows = value if value and isinstance(value[0], list) else [value]
        if not rows or any(not isinstance(row, list) or len(row) != len(rows[0]) for row in rows):
            raise ValueError("nonrectangular array")
        if any(not isinstance(v, str) or not BITS.fullmatch(v) for row in rows for v in row):
            raise ValueError("array contains nonnumeric values")
        return number_array([[v[2:] for v in row] for row in rows])
    raise ValueError("unsupported argument")


def without_mode(value):
    if isinstance(value, list): return [without_mode(v) for v in value]
    if isinstance(value, dict): return {k: without_mode(v) for k, v in value.items() if k != "mode"}
    return value


def answer(value):
    kind = value.get("type")
    if kind == "number": return "0x" + value["bits"].lower()
    if kind == "text": return "text:" + value["value"]
    if kind == "boolean": return "logical:" + str(value["value"])
    if kind == "error" and value["code"] in ERRORS: return "error:" + ERRORS[value["code"]]
    return None


def select_spill(value, index):
    """Apply the witness selection to a whole-array answer, including zero axes."""
    if not isinstance(index, (list, tuple)) or len(index) != 2:
        return None
    if any(not isinstance(i, int) or isinstance(i, bool) or i < 0 for i in index):
        return None
    r, c = (max(i, 1) - 1 for i in index)
    if value.get("type") != "array":
        return value if (r, c) == (0, 0) else None
    rows = value.get("values")
    if not isinstance(rows, list) or len(rows) != value.get("rows"):
        return None
    if any(not isinstance(row, list) or len(row) != value.get("cols") for row in rows):
        return None
    if r >= len(rows) or c >= len(rows[r]):
        return None
    return rows[r][c]


def use_whole_array(row, enabled):
    index = row.get("result_index")
    return bool(enabled and row.get("function") == "MINVERSE" and index
                and len(index) == 2 and 0 in index)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("export", type=pathlib.Path)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--batch", type=int, default=5000)
    parser.add_argument("--inflight", type=int, default=4)
    parser.add_argument("--priority", choices=("bulk", "interactive"), default="bulk")
    parser.add_argument("--whole-array-zero-axis", action="store_true",
                        help="read a whole MINVERSE spill for a selection with a zero axis")
    opts = parser.parse_args()
    if not 1 <= opts.batch < 20000 or not 1 <= opts.inflight <= 16:
        parser.error("batch must be 1..19999, inflight 1..16")
    opts.output.mkdir(parents=True, exist_ok=False)
    client = Client(transport="socket", timeout=900)
    conn = client._socket()
    active = []
    buckets = collections.defaultdict(list)
    stats = collections.defaultdict(collections.Counter)
    waits = []
    calls = 0
    started = time.monotonic()
    with gzip.open(opts.output / "exceptions.jsonl.gz", "wt", encoding="utf8") as exceptions, \
         (opts.output / "batches.jsonl").open("w", encoding="utf8") as batches:
        def record(row, category, **extra):
            stats[row["function"]][category] += 1
            exceptions.write(json.dumps({"category": category, "bank": row, **extra}, ensure_ascii=True, separators=(",", ":")) + "\n")

        def finish(pending, rows):
            try:
                reply = pending.result(timeout=30)
                failure = None
            except Exception as exc:
                reply, failure = None, str(exc)
            duration = pending.seconds
            if duration > 3:
                waits.append({"request": pending.rid, "seconds": duration, "calls": len(rows)})
                conn.report_wait("witness drift request", duration, kind="transport", lane="drift1")
            envs = {eid: conn.envs.get(eid, {}).get("env") for eid in pending.env_ids}
            if failure is not None:
                batches.write(json.dumps({"id": pending.rid, "function": rows[0][0]["function"],
                    "calls": len(rows), "seconds": duration, "environments": envs,
                    "failure": failure}, separators=(",", ":")) + "\n")
                for row, _ in rows: record(row, "service_failure", detail=failure)
                return
            if len(reply.get("rows", [])) != len(rows) or len(reply.get("inputs", [])) != len(rows):
                for row, _ in rows: record(row, "incomplete_reply", detail=pending.rid)
                return
            batches.write(json.dumps({"id": pending.rid, "function": rows[0][0]["function"], "calls": len(rows),
                                      "seconds": duration, "environments": envs}, separators=(",", ":")) + "\n")
            for pos, ((row, requested), observed) in enumerate(zip(rows, reply["rows"])):
                env = envs.get(pending.env_by_position[pos]) or {}
                readback = reply.get("inputs", [])[pos]
                if env.get("build") != "20527":
                    record(row, "wrong_build", observed=observed, inputs=readback, environment=env)
                elif observed.get("type") == "unread" or without_mode(readback) != without_mode(requested):
                    record(row, "input_changed", observed=observed, inputs=readback, environment=env)
                else:
                    whole = use_whole_array(row, opts.whole_array_zero_axis)
                    selected = select_spill(observed, row["result_index"]) if whole else observed
                    actual = answer(selected) if selected is not None else None
                    evidence = {"observed": selected, "inputs": readback, "environment": env}
                    if whole:
                        evidence["whole_array"] = observed
                    if actual is None:
                        record(row, "unrecognised_answer", **evidence)
                    elif actual != row["expected"]:
                        record(row, "different", **evidence)
                    else:
                        stats[row["function"]]["agree"] += 1

        def submit(key):
            nonlocal calls
            rows = buckets.pop(key)
            try:
                pending = conn.submit_rows(key[0], [r[1] for r in rows],
                                           result_index=None if key[2] else key[1],
                                           priority=opts.priority, echo=True, spill=key[2])
            except (ValueError, TypeError) as exc:
                for row, _ in rows: record(row, "unsupported_request", detail=str(exc))
                return
            calls += len(rows)
            active.append((pending, rows))
            if len(active) >= opts.inflight:
                p, r = active.pop(0)
                finish(p, r)

        with opts.export.open(encoding="utf8") as source:
            for line in source:
                row = json.loads(line)
                stats[row["function"]]["bank_rows"] += 1
                if row.get("exclusion"):
                    record(row, "excluded", reason=row["exclusion"])
                    continue
                try:
                    requested = [arg(v) for v in row["args"]]
                except ValueError as exc:
                    record(row, "unsupported_input", detail=str(exc))
                    continue
                index = tuple(row["result_index"]) if row["result_index"] is not None else None
                key = (row["function"], index, use_whole_array(row, opts.whole_array_zero_axis))
                buckets[key].append((row, requested))
                if len(buckets[key]) == opts.batch:
                    submit(key)
        for key in list(buckets): submit(key)
        while active:
            p, r = active.pop(0)
            finish(p, r)
    elapsed = time.monotonic() - started
    summary = {"wall_seconds": elapsed, "calls": calls, "calls_per_second": calls / elapsed,
               "waits_over_3_seconds": waits, "functions": stats}
    (opts.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf8")
    client.close()
    print(json.dumps({k: v for k, v in summary.items() if k != "functions"}, indent=2))


if __name__ == "__main__":
    main()
