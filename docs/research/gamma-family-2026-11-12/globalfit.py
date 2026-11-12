# Global fit of the B3 table inside a fixed graph: compensated pair moves (the output at a chosen x0 is
# kept to first order) scored on exact matches over a subsample of every E1 window.
import numpy as np, sim, load, json, sys, itertools, time
xs, E = load.load(); W = len(xs) // 1024
mode, src, out = int(sys.argv[1]), sys.argv[2], sys.argv[3]
per = int(sys.argv[4]) if len(sys.argv) > 4 else 128
sel = np.concatenate([np.arange(w * 1024, w * 1024 + 1024, 1024 // per) for w in range(256)])
xx = np.ascontiguousarray(xs[sel]); ee = E[sel]
C = np.array([int(h, 16) for h in json.load(open(src))], dtype=np.uint64).view(np.float64).copy()
def score(C): return int((sim.b3(C[:9], C[9:], xx, mode) == ee).sum())
def step(C, i, m):
    Cn = C.copy(); Cn[i] = (np.array([C[i]]).view(np.int64) + int(m)).view(np.float64)[0]; return Cn
def gains(C, x0):
    P, Q = C[:9], C[9:]
    def pv(c):
        v = c[8]
        for k in range(7, -1, -1): v = v * x0 + c[k]
        return v
    N, D = pv(P), pv(Q)
    return np.array([((x0 - 2) * x0 ** (i % 9) * np.spacing(abs(C[i])) / D) if i < 9 else
                     (-(x0 - 2) * N * x0 ** (i % 9) * np.spacing(abs(C[i])) / D ** 2) for i in range(18)])
best = score(C); n = len(xx); print("start %.4f" % (best / n), flush=True); t = time.time()
# P0/Q0 valley: Q0 by its grid, P0 compensating at x0 = 2.25
for sweep in range(int(sys.argv[5]) if len(sys.argv) > 5 else 6):
    imp = 0
    for x0 in (2.5, 2.1, 2.9, 2.3, 2.7, 2.02):
        g = gains(C, x0)
        for i, j in itertools.permutations(range(18), 2):
            if g[j] == 0: continue
            for mi in (1, 2, 4, 8, 16, 32, 64, 128, 512, 2048, 8192):
                for sgn in (1, -1):
                    m = sgn * mi; mj0 = -m * g[i] / g[j]
                    if abs(mj0) > 1e7: continue
                    for e in (-1, 0, 1):
                        Cn = step(step(C, i, m), j, int(round(mj0)) + e); s = score(Cn)
                        if s > best:
                            best = s; C = Cn; imp += 1
        print(sweep, "x0", x0, "%.4f" % (best / n), "%.0fs" % (time.time() - t), flush=True)
        json.dump([sim.hx(v) for v in C], open(out, "w"))
    if not imp: break
print("final %.4f" % (best / n))
