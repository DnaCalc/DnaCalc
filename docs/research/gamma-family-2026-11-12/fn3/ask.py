"""Batched Excel probe via Touchstone. ask(func, rows) where rows are lists of float or hex-bit strings.
Returns list of (letter, value) per row."""
import sys, struct, json
sys.path.insert(0, __import__("os").path.join(__import__("os").path.dirname(__file__), "../../../../tools/excel-service/touchstone"))
import client as tc

def hx(x):
    if isinstance(x, str): return x
    return "%016x" % struct.unpack("<Q", struct.pack("<d", float(x)))[0]

def fl(h):
    return struct.unpack("<d", struct.pack("<Q", int(h, 16)))[0]

_conn = None
def conn():
    global _conn
    if _conn is None:
        _conn = tc.Connection.open(name="fn3")
        _conn.wait_hello()
    return _conn

def ask(func, rows, kinds=None, chunk=50000, timeout=3600):
    rows = [[hx(a) if not isinstance(a, bool) else a for a in r] for r in rows]
    width = len(rows[0])
    k = kinds or ",".join(["n"] * width)
    out = []
    pend = []
    for i in range(0, len(rows), chunk):
        part = rows[i:i+chunk]
        g = {"f": func, "k": k, "n": len(part), "v": [v for r in part for v in r]}
        pend.append(conn().submit_groups([g], priority="bulk"))
    for p in pend:
        L, V = p.raw(timeout)
        out.extend(zip(L, V))
    return out
