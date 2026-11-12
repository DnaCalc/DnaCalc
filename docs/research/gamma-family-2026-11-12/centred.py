# Is B3 a Horner scheme in a centred variable t = x - a? Convert the landed raw-x table exactly to the t
# basis, round each coefficient to binary64 and evaluate with every stage stored.
import numpy as np, sim, load, sys
from fractions import Fraction as Fr
from math import comb
L = np.longdouble
xs, E = load.load(); W = len(xs) // 1024
X = xs.reshape(W, 1024); EE = E.reshape(W, 1024); cx = X[:, 512]
P = [Fr(v) for v in sim.f(sim.B3_P)]; Q = [Fr(v) for v in sim.f(sim.B3_Q)]
def shift(c, a):
    # p(x) = sum c_i x^i ; x = t + a -> coefficients in t
    out = [Fr(0)] * len(c)
    for i, ci in enumerate(c):
        for j in range(i + 1): out[j] += ci * comb(i, j) * a ** (i - j)
    return out
def horner_t(c, t, store_prod=True, store_last=True):
    v = np.full(len(t), float(c[8]))
    for k in range(7, -1, -1):
        p = (v.astype(L) * t.astype(L))
        if store_prod: p = p.astype(np.float64).astype(L)
        s = p + L(float(c[k]))
        if k > 0 or store_last: s = s.astype(np.float64)
        v = s
    return v.astype(L) if not isinstance(v, np.ndarray) or v.dtype != L else v
def evaluate(a, store_prod, store_last, ws):
    Pt = shift(P, Fr(a)); Qt = shift(Q, Fr(a))
    x = np.ascontiguousarray(X[ws].ravel()); e = EE[ws].ravel()
    t = x - a
    assert (t + a == x).all()
    n = horner_t(Pt, t, store_prod, store_last); d = horner_t(Qt, t, store_prod, store_last)
    r = (n.astype(L) / d.astype(L)).astype(np.float64)
    y = (r.astype(L) * (x - 2.0).astype(L)).astype(np.float64)
    return (y == e).reshape(len(ws), 1024).mean(1)
ws = list(range(0, 256, 4)) + [256, 257]
for a in (2.0, 2.25, 2.5, 2.75, 3.0):
    for sp in (True, False):
        for sl in (True, False):
            m = evaluate(a, sp, sl, ws)
            near = [i for i, w in enumerate(ws) if abs(cx[w] - 2.5) < 0.03]
            print("a=%.2f store_prod=%d store_last=%d  all %.3f  near2.5 %.3f  at2.75 %.3f" % (a, sp, sl, m.mean(), m[near].mean(), m[-1]))
