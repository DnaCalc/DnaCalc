r"""WP5 proofs against a running broker.

  pairs     the WEEKNUM paired readings (8 serials x 10 prior types x 12 invalid types) in serial mode
  leak      a 100k mixed sample through the reversed-order check (run the broker with --registry none --leak-every 1)
  spill     array-returning questions: TEXTSPLIT, MINVERSE, a #SPILL! answer

  py -3 proof_wp5.py pairs|leak|spill --state C:\Work\DnaCalc\codex-runs\excel-service-v2
"""
import argparse
import json
from pathlib import Path
import struct
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))

import client as top  # noqa: E402
import protocol  # noqa: E402

SERIALS = [43100.0, 43836.0, 44000.0, 44197.0, 44562.0, 45000.0, 45292.0, 45657.0]
PRIOR = [1, 2, 3, 10, 11, 12, 16, 17, 18, 21]
INVALID = [-1, -2, 22, 23, 24, 30, 99, 100, 256, 1000, 65536, 2147483648]
BITS = lambda x: format(struct.unpack(">Q", struct.pack(">d", float(x)))[0], "016x")  # noqa: E731


def expected_after(prior, valid):
    """The rule measured on 2026-11-08 (sweep-questions-1 Q3): which valid type an invalid one answers as."""
    if prior in (1, 17):
        return valid[1]
    if prior in (2, 11, 21):
        return valid[2]
    return None          # #NUM!


def pairs(options):
    client = top.Client(timeout=300)
    base = client.ask("WEEKNUM", [[top.number(s), top.number(float(t))] for s in SERIALS for t in (1, 2)])
    valid = {s: {1: base["rows"][2 * i]["bits"], 2: base["rows"][2 * i + 1]["bits"]} for i, s in enumerate(SERIALS)}
    rows, expect = [], []
    for s in SERIALS:
        for prior in PRIOR:
            for invalid in INVALID:
                rows.append([top.number(s), top.number(float(prior))])
                expect.append(("prior", s, prior))
                rows.append([top.number(s), top.number(float(invalid))])
                expect.append(("invalid", s, prior, invalid))
    started = time.monotonic()
    reply = client.ask("WEEKNUM", rows, order="serial")
    seconds = time.monotonic() - started
    ok = bad = 0
    examples = []
    for answer, e in zip(reply["rows"], expect):
        if e[0] != "invalid":
            continue
        want = expected_after(e[2], valid[e[1]])
        good = (answer.get("code") == "#NUM!") if want is None else (answer.get("bits") == want)
        ok += good
        if not good:
            bad += 1
            if len(examples) < 5:
                examples.append({"serial": e[1], "prior": e[2], "invalid": e[3], "answer": answer, "want": want})
    report = {"pairs": len(rows) // 2, "calls": len(rows), "match_the_rule": ok, "differ": bad, "seconds": round(seconds, 2),
              "order": reply.get("order"), "differing_examples": examples, "valid_types_checked": valid[SERIALS[0]]}
    report["note"] = "serial mode reproduces every paired reading" if bad == 0 else "serial mode differs on some pairs"
    print(json.dumps(report, indent=1))
    return report


def mixed_sample(state, total=100000):
    sample = json.loads((state / "wp0-sample.json").read_text())
    groups = [g for g in sample["groups"] if g["f"] != "WEEKNUM"]
    out, calls = [], 0
    while calls < total:
        for g in groups:
            out.append(g)
            calls += g["n"]
            if calls >= total:
                break
    week = []
    for s in SERIALS:
        for prior in PRIOR:
            for invalid in INVALID:
                week.append((s, prior))
                week.append((s, invalid))
    wk = {"f": "WEEKNUM", "k": "n,n", "n": len(week), "v": [BITS(x) for pair in week for x in pair]}
    return protocol.with_positions([*out, wk]), calls + len(week)


def leak(options):
    state = options.state
    groups, total = mixed_sample(state)
    conn = top.Client()._socket()
    before = len((state / "log" / "leakage.jsonl").read_text().splitlines()) if (state / "log" / "leakage.jsonl").exists() else 0
    pending = conn.submit_groups(groups, priority="bulk")
    pending.raw(600)
    deadline = time.monotonic() + 180
    while True:         # the checks run on idle workers after the answers
        try:
            status = json.loads((state / "status.json").read_text())
        except (OSError, ValueError):
            time.sleep(0.2)
            continue
        if status.get("pending_leak_checks", 0) == 0 or time.monotonic() > deadline:
            time.sleep(2)
            status = json.loads((state / "status.json").read_text())
            break
        time.sleep(0.5)
    lines = (state / "log" / "leakage.jsonl").read_text().splitlines()[before:]
    records = [json.loads(line) for line in lines]
    by_function = {}
    invalid_only = True
    for r in records:
        by_function[r["function"]] = by_function.get(r["function"], 0) + 1
        if r["function"] == "WEEKNUM":
            t = struct.unpack(">d", bytes.fromhex(r["args"][1]))[0]
            if not (t < 0 or t >= 22):
                invalid_only = False
    report = {"calls": total, "reversed_checks": status["leak_checks"], "flagged_by_function": by_function,
              "weeknum_flagged_calls_all_invalid_types": invalid_only,
              "example": records[0] if records else None}
    print(json.dumps(report, indent=1))
    return report


def spill(options):
    client = top.Client(timeout=120)
    out = {}
    e_acute = "\u00e9"
    r = client.ask("TEXTSPLIT", [[top.text("e-E-e".replace("e", e_acute) if False else "e-E-e"), top.text("e")]], shape="array")
    out["TEXTSPLIT(e-E-e, e)"] = r["rows"][0]
    m = [[4.0, 3.0, 2.0, 1.0], [1.0, 3.0, 2.0, 5.0], [2.0, 1.0, 6.0, 1.0], [1.0, 1.0, 1.0, 9.0]]
    r = client.ask("MINVERSE", [[top.number_array(m)]], shape="array")
    out["MINVERSE 4x4"] = {"rows": r["rows"][0]["rows"], "cols": r["rows"][0]["cols"],
                           "first_row": [c.get("bits") for c in r["rows"][0]["values"][0]]}
    r = client.ask("SEQUENCE", [[top.number(3.0), top.number(2.0)]], shape="array")
    out["SEQUENCE(3,2)"] = [[c.get("bits") for c in row] for row in r["rows"][0]["values"]]
    r = client.ask("TRANSPOSE", [[top.number_array([[1.0, 2.0, 3.0]])]], shape="array")
    out["TRANSPOSE 1x3"] = [[c.get("bits") for c in row] for row in r["rows"][0]["values"]]
    # arguments never sit inside the spill range: no #SPILL! from the layout
    r = client.ask("SEQUENCE", [[top.number(100.0), top.number(100.0)]], shape="array")
    out["SEQUENCE(100,100)"] = r["rows"][0].get("type") + " " + str(r["rows"][0].get("reason", r["rows"][0].get("code")))
    print(json.dumps(out, indent=1))
    return out


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["pairs", "leak", "spill"])
    parser.add_argument("--state", type=Path, default=Path(r"C:\Work\DnaCalc\codex-runs\excel-service-v2"))
    options = parser.parse_args()
    {"pairs": pairs, "leak": leak, "spill": spill}[options.command](options)
