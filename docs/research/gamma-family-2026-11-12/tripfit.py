# Triple moves that keep the output unchanged to first order at two points, plus compensated pairs at
# many points and single moves; scored on exact matches over a subsample of every E1 window.
import numpy as np, sim, load, json, sys, itertools, time, random
xs, E = load.load(); W = len(xs) // 1024
mode, src, out = int(sys.argv[1]), sys.argv[2], sys.argv[3]
per = int(sys.argv[4]) if len(sys.argv) > 4 else 64
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
rng = random.Random(1)
pts = [2.05, 2.15, 2.25, 2.35, 2.45, 2.55, 2.65, 2.75, 2.85, 2.95]
for sweep in range(40):
    imp = 0
    a, b = rng.sample(pts, 2)
    ga, gb = gains(C, a), gains(C, b)
    for i, j, k in itertools.permutations(range(18), 3):
        if j > k: continue
        M = np.array([[ga[j], ga[k]], [gb[j], gb[k]]])
        if abs(np.linalg.det(M)) < 1e-300: continue
        for mi in (1, 4, 16, 64, 256, 1024):
            for sgn in (1, -1):
                m = sgn * mi
                try: sol = np.linalg.solve(M, -m * np.array([ga[i], gb[i]]))
                except np.linalg.LinAlgError: continue
                if np.abs(sol).max() > 1e7: continue
                Cn = step(step(step(C, i, m), j, int(round(sol[0]))), k, int(round(sol[1]))); s = score(Cn)
                if s > best: best = s; C = Cn; imp += 1
    x0 = rng.choice(pts); g = gains(C, x0)
    for i, j in itertools.permutations(range(18), 2):
        for mi in (1, 2, 4, 8, 16, 32, 64, 128, 512, 2048, 8192):
            for sgn in (1, -1):
                m = sgn * mi; mj0 = -m * g[i] / g[j]
                if abs(mj0) > 1e7: continue
                for e in (-1, 0, 1):
                    Cn = step(step(C, i, m), j, int(round(mj0)) + e); s = score(Cn)
                    if s > best: best = s; C = Cn; imp += 1
    print(sweep, "pts", a, b, x0, "%.4f" % (best / n), "moves", imp, "%.0fs" % (time.time() - t), flush=True)
    json.dump([sim.hx(v) for v in C], open(out, "w"))
