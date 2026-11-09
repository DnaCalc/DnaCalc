#!/usr/bin/env python3
"""Summarises one raw run of volatility-job.ps1 (vol-1).

  summarize.py RAW_DIR SPEC_NAME [--tags s1,s2]  -> text table on stdout

Per formula cell A<i> (dependent counter B<i> = Cnt(A<i>&"")) and per trial: the counter's calls in each
clean Calculate ("1" called, "0" not), the formula's own user-function calls per clean Calculate, and,
from each saved .xlsb, the leading PtgAttrSemi token (tok) and the cell's always-calculate bit (ca).
Trials that disagree are listed side by side."""
import json, os, re, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import readout

def parse_counts(text):
    d = {}
    for item in (text or "").split(";"):
        if "=" in item:
            k, v = item.rsplit("=", 1)
            d[k] = d.get(k, 0) + int(v)
    return d

def parse_xll(text):
    """name#k@row,col t.. :T=0,F=0,;  ->  list of (name, row, col, rest)"""
    out = []
    for item in (text or "").split(";"):
        m = re.match(r"(\w+)#(\d+)@(\d+),(\d+)(.*)$", item)
        if m:
            out.append((m.group(1), int(m.group(3)), int(m.group(4)), m.group(5)))
    return out

def cell_row(addr):
    return int(re.sub(r"\D", "", addr))

def main():
    raw = sys.argv[1]
    name = sys.argv[2]
    tags = None
    if "--tags" in sys.argv:
        tags = sys.argv[sys.argv.index("--tags") + 1].split(",")
    data = json.load(open(os.path.join(raw, f"volatility-{name}.json"), encoding="utf-8-sig"))
    print(f"# {name}: Excel {data['excel_version']} build {data['excel_build']}, CalculationVersion {data['calculation_version']}, "
          f"MTC {data['mtc_enabled']}/{data['mtc_threads']}, started {data['started_utc']}")
    for e in data.get("errors", []):
        print("ERROR", e)
    by_probe = {}
    for row in data["rows"]:
        by_probe.setdefault(row["probe"], []).append(row)
    for probe, rows in by_probe.items():
        print(f"\n## probe {probe}: {len(rows)} trials")
        for r in rows:
            if r.get("error"):
                print(f"  trial {r['trial']} ERROR at {r.get('step')}: {r['error']}")
            for k in ("entry_errors", "name_errors"):
                if r.get(k):
                    print(f"  trial {r['trial']} {k}: {r[k]}")
        formulas = {}
        for r in rows:
            for f in r.get("formulas", []):
                a, _, text = f.partition(" ")
                formulas[a] = text
        saves = [t for t in rows[0].get("saves", [])] if rows else []
        saves = [s[:-5] if s.endswith(".xlsx") else s for s in saves]
        if tags:
            saves = [s for s in saves if s in tags]
        saves = [s for s in dict.fromkeys(saves) if not s.endswith(".xlsx")]
        # per trial per cell
        table = {}   # addr -> list per trial of dict
        for r in rows:
            clean = [rec for rec in r["records"] if rec["tag"].startswith(("clean", "s1c", "s2c", "s3c"))]
            tdir = os.path.join(raw, f"{probe}_t{r['trial']}")
            reads = {}
            for s in saves:
                path = os.path.join(tdir, s + ".xlsb")
                if os.path.exists(path):
                    reads[s] = readout.read(path)
            for a in formulas:
                if not a.startswith("A"):
                    continue
                i = cell_row(a)
                ent = {"ctr": [], "udf": [], "tok": [], "ca": []}
                for rec in clean:
                    cnt = parse_counts(rec["counts"])
                    ent["ctr"].append(cnt.get(f"Cnt@B{i}", 0))
                    udf = sum(v for k, v in cnt.items() if k.endswith(f"@A{i}"))
                    xl = sum(1 for (n, rr, cc, rest) in parse_xll(rec.get("xll")) if rr == i and cc == 1)
                    ent["udf"].append(udf + xl)
                for s in saves:
                    sheet = reads.get(s, {}).get("sheet1", {})
                    c = sheet.get(a, {})
                    tok = "Y" if (c.get("token") or c.get("arr", {}).get("token")) else "n"
                    ent["tok"].append(tok)
                    ent["ca"].append(str(c.get("ca", "-")))
                table.setdefault(a, []).append(ent)
        order = sorted(table, key=cell_row)
        head = "cell | formula | counter per clean | own UDF calls per clean | " + " | ".join(f"{s}: tok,ca" for s in saves)
        print(head)
        for a in order:
            variants = {}
            for ent in table[a]:
                key = ("".join(str(min(x, 9)) for x in ent["ctr"]),
                       ",".join(str(x) for x in ent["udf"]),
                       tuple((t, c) for t, c in zip(ent["tok"], ent["ca"])))
                variants[key] = variants.get(key, 0) + 1
            parts = []
            for (ctr, udf, tc), n in variants.items():
                parts.append(f"{ctr} | {udf} | " + " | ".join(f"{t},{c}" for t, c in tc) + (f"  (x{n})" if len(variants) > 1 or n < len(table[a]) else ""))
            print(f"{a} | {formulas[a]} | " + "  //  ".join(parts))
        names = {}
        for r in rows[:1]:
            for s in saves:
                path = os.path.join(raw, f"{probe}_t{r['trial']}", s + ".xlsb")
                if os.path.exists(path):
                    nm = readout.read(path).get("names")
                    if nm:
                        names[s] = {k: ("semi" if v["token"] else "-") + ":" + v["first"] for k, v in nm.items() if k.startswith("Nm")}
        if names:
            print("names (first trial): " + json.dumps(names))

if __name__ == "__main__":
    main()
