# Relative error of Excel's GAMMALN against the truth on the B1 and B2 bands, from the bank.
import json, struct, sys, math
import numpy as np
sys.path.insert(0, ".")
import hpgamma
from decimal import Decimal as D
from fractions import Fraction as Fr
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def dec(v): f = Fr(v); return D(f.numerator) / D(f.denominator)
pts = []
for l in open(sys.argv[1]):
    r = json.loads(l); x = fl(r["args"][0])
    lo, hi = float(sys.argv[2]), float(sys.argv[3])
    if not (lo <= x < hi) or abs(x - 1) < 1e-6 or abs(x - 2) < 1e-6: continue
    t = hpgamma.lgamma_hp(x)
    e = fl(r["expected"]); a = fl(r["actual"])
    rel_e = float((dec(e) - t) / t) / 2 ** -53
    rel_a = float((dec(a) - t) / t) / 2 ** -53
    pts.append((x, rel_e, rel_a))
pts.sort()
xs = np.array([p[0] for p in pts]); re = np.array([p[1] for p in pts]); ra = np.array([p[2] for p in pts])
nb = int(sys.argv[4]) if len(sys.argv) > 4 else 32
edges = np.linspace(lo, hi, nb + 1)
for i in range(nb):
    m = (xs >= edges[i]) & (xs < edges[i + 1])
    if m.sum() < 5: continue
    print("%.4f-%.4f n=%4d  Excel %+6.2f (se %.2f)  ours %+6.2f   [units of 2^-53 relative]" % (edges[i], edges[i + 1], m.sum(), re[m].mean(), re[m].std() / math.sqrt(m.sum()), ra[m].mean()))
