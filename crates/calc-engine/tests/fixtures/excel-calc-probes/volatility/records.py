#!/usr/bin/env python3
"""Per-record view of a raw run: for each trial and record, the user-function calls (VBA name@caller=n) and
the dependent counters that ran, plus the XLL log and any returned text.   records.py RAW_DIR NAME [TRIAL]"""
import json, sys
raw, name = sys.argv[1], sys.argv[2]
only = int(sys.argv[3]) if len(sys.argv) > 3 else None
d = json.load(open(f"{raw}/volatility-{name}.json", encoding="utf-8-sig"))
for r in d["rows"]:
    if only and r["trial"] != only:
        continue
    print(f"## trial {r['trial']} error={r.get('error')} step={r.get('step')}")
    if "formulas" in r:
        print("formulas:", "; ".join(f for f in r["formulas"] if not f.split(" ")[0].startswith("B")))
    for rec in r["records"]:
        c = rec.get("counts") or ""
        udf = [i for i in c.split(";") if i and not i.startswith("Cnt@")]
        ctr = sorted(i.split("@")[1].split("=")[0] for i in c.split(";") if i.startswith("Cnt@"))
        extra = {k: rec[k] for k in ("ret", "expr", "macro", "name") if k in rec}
        print(rec["tag"], rec["op"], "| udf:", " ".join(udf), "| ctr:", " ".join(ctr), "|", (rec.get("xll") or "")[:300], extra or "")
