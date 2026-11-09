#!/usr/bin/env python3
"""Collects raw runs into results/ (small files): collect.py RAW_ROOT
For each probe directory RAW_ROOT/<name>: results/<name>.json.gz (the job's raw rows), results/<name>-readout.json.gz
(readout.py of every saved .xlsb, per trial and save tag) and results/<name>.txt (summarize.py table)."""
import gzip, json, os, re, shutil, subprocess, sys
here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
import readout
root = sys.argv[1]
out = os.path.join(here, "results")
os.makedirs(out, exist_ok=True)
for name in sorted(os.listdir(root)):
    d = os.path.join(root, name)
    base = re.sub(r"r\d$", "", name)   # x4r2 is a repeat run of x4
    jf = os.path.join(d, f"volatility-{base}.json")
    if not os.path.isdir(d) or not os.path.exists(jf):
        continue
    with open(jf, "rb") as f, gzip.open(os.path.join(out, name + ".json.gz"), "wb") as g:
        shutil.copyfileobj(f, g)
    reads = {}
    for t in sorted(os.listdir(d)):
        td = os.path.join(d, t)
        if os.path.isdir(td):
            for fn in sorted(os.listdir(td)):
                if fn.endswith((".xlsb", ".xlsx")) and fn != "book.xlsb":
                    reads[f"{t}/{fn}"] = readout.read(os.path.join(td, fn))
    with gzip.open(os.path.join(out, name + "-readout.json.gz"), "wt") as g:
        json.dump(reads, g, separators=(",", ":"), sort_keys=True)
    pname = base
    txt = subprocess.run([sys.executable, os.path.join(here, "summarize.py"), d, pname], capture_output=True, text=True).stdout
    open(os.path.join(out, name + ".txt"), "w").write(txt)
    print(name, os.path.getsize(os.path.join(out, name + ".json.gz")), os.path.getsize(os.path.join(out, name + "-readout.json.gz")))
