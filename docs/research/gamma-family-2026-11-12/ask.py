import sys, json, time
sys.path.insert(0, "/home/koderbot/projects/DnaCalc/DnaCalc-opus/tools/excel-service")
from client import Client, number
def ask_bits(function, rows, chunk=100000):
    """rows: list of lists of hex args; returns list of result dicts"""
    c = Client(); out = []
    reqs = [(function, [[number(a) for a in r] for r in rows[i:i+chunk]]) for i in range(0, len(rows), chunk)]
    reps = c.ask_many(reqs)
    meta = None
    for rep in reps:
        out.extend(rep["rows"]); meta = (rep["excel_build"], rep.get("env_id"))
    return out, meta
