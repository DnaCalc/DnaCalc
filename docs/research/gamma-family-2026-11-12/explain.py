# For mismatched rows: which single stored stage, moved by one unit, gives Excel's answer?
import numpy as np, sim, load, json, sys, collections
L = np.longdouble
xs, E = load.load(); W = len(xs) // 1024
X = xs.reshape(W, 1024); EE = E.reshape(W, 1024); cx = X[:, 512]
lo, hi, mode, src = float(sys.argv[1]), float(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
C = np.array([int(h, 16) for h in json.load(open(src))], dtype=np.uint64).view(np.float64)
ws = [w for w in range(W) if lo < cx[w] < hi]
x = np.ascontiguousarray(X[ws].ravel()); e = EE[ws].ravel()
def nxt(v, k):
    b = v.view(np.int64) + np.where(v >= 0, k, -k); return b.view(np.float64)
def run(pert=None, k=0):
    names = []
    def st(v, name):
        d = v.astype(np.float64)
        names.append(name)
        if pert == name: d = nxt(d, k)
        return d
    def chain(c, tag, store_last):
        v = np.full(len(x), c[8])
        for kk in range(7, 0, -1):
            p = st(v.astype(L) * x.astype(L), tag + "p%d" % (kk + 1)); v = st(p.astype(L) + L(c[kk]), tag + "s%d" % kk)
        p = st(v.astype(L) * x.astype(L), tag + "p1"); s = p.astype(L) + L(c[0])
        return st(s, tag + "s0").astype(L) if store_last else s
    N = chain(C[:9], "P", bool(mode & 5)); D = chain(C[9:], "Q", bool(mode & 9))
    r = st(N / D, "R"); y = st(r.astype(L) * (x - 2.0).astype(L), "y")
    return y, names
y0, names = run()
assert (y0 == sim.b3(C[:9], C[9:], x, mode)).all()
bad = y0 != e
print("rows", len(x), "mismatched", bad.sum())
cnt = collections.Counter(); explained = np.zeros(len(x), bool)
for nm in names:
    for k in (1, -1):
        y1, _ = run(nm, k)
        hit = bad & (y1 == e)
        cnt[(nm, k)] = hit.sum(); explained |= hit
for (nm, k), v in cnt.most_common(16): print("%5s %+d explains %d" % (nm, k, v))
print("explained by some single stage:", explained.sum(), "of", bad.sum())
