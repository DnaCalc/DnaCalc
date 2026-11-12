# Pair moves that leave the output at x = 2.5 unchanged to first order (compensated by the gains at 2.5),
# scored on exact matches over a region around 2.5. Larger steps than the earlier local searches.
import numpy as np, sim, load, json, sys, itertools, time, math
xs, E = load.load(); W = len(xs) // 1024
X = xs.reshape(W, 1024); EE = E.reshape(W, 1024); cx = X[:, 512]
lo, hi, mode, src, out = float(sys.argv[1]), float(sys.argv[2]), int(sys.argv[3]), sys.argv[4], sys.argv[5]
ws = [w for w in range(W) if lo < cx[w] < hi]
xx = np.ascontiguousarray(X[ws].ravel()); ee = EE[ws].ravel()
C = np.array([int(h, 16) for h in json.load(open(src))], dtype=np.uint64).view(np.float64).copy()
def score(C):
    d = sim.ulps(sim.b3(C[:9], C[9:], xx, mode), ee); return int((d == 0).sum())
def step(C, i, m):
    Cn = C.copy(); Cn[i] = (np.array([C[i]]).view(np.int64) + int(m)).view(np.float64)[0]; return Cn
x0 = float(sys.argv[6]) if len(sys.argv) > 6 else 2.5
def gains(C):
    P, Q = C[:9], C[9:]
    def pv(c, x):
        v = c[8]
        for k in range(7, -1, -1): v = v * x + c[k]
        return v
    N, D = pv(P, x0), pv(Q, x0)
    g = []
    for i in range(18):
        k = i % 9
        dv = x0 ** k * np.spacing(abs(C[i]))
        g.append((x0 - 2) * dv / D if i < 9 else -(x0 - 2) * N * dv / D ** 2)
    return np.array(g)       # output change per ulp of each coefficient
best = score(C); print("start %.4f" % (best / len(xx)), flush=True); t = time.time()
for sweep in range(6):
    imp = 0
    g = gains(C)
    for i, j in itertools.permutations(range(18), 2):
        if g[j] == 0: continue
        for mi in (1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128):
            for sgn in (1, -1):
                m = sgn * mi
                mj0 = -m * g[i] / g[j]
                if abs(mj0) > 1e6: continue
                for e in (-1, 0, 1):
                    mj = int(round(mj0)) + e
                    Cn = step(step(C, i, m), j, mj); s = score(Cn)
                    if s > best:
                        best = s; C = Cn; imp += 1
                        print(sweep, i, j, m, mj, "%.4f" % (best / len(xx)), "%.0fs" % (time.time() - t), flush=True)
    json.dump([sim.hx(v) for v in C], open(out, "w"))
    if not imp: break
print("final %.4f" % (best / len(xx)))
