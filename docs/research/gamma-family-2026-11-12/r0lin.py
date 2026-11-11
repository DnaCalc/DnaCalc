# Inside one binade region every stage grid is fixed, so moving a coefficient by whole grid steps moves
# the window means linearly. Solve the region's window means for integer grid steps (truncated SVD),
# then keep the move if the exact-match count improves.
import numpy as np, sim, load, json, sys, math
xs, E = load.load(); W = len(xs) // 1024
X = xs.reshape(W, 1024); EE = E.reshape(W, 1024); cx = X[:, 512]
lo, hi, mode, src, out = float(sys.argv[1]), float(sys.argv[2]), int(sys.argv[3]), sys.argv[4], sys.argv[5]
ws = [w for w in range(W) if lo < cx[w] < hi]
C = np.array([int(h, 16) for h in json.load(open(src))], dtype=np.uint64).view(np.float64).copy()
xx = np.ascontiguousarray(X[ws].ravel()); ee = EE[ws].ravel(); nw = len(ws)
def sim_(C): return sim.b3(C[:9], C[9:], xx, mode)
def means(C): return sim.ulps(ee, sim_(C)).reshape(nw, 1024).mean(1)
def match(C): return (sim_(C) == ee).mean()
def step(C, i, m):
    Cn = C.copy(); Cn[i] = (np.array([C[i]]).view(np.int64) + int(m)).view(np.float64)[0]; return Cn
G = [int(v) for v in json.loads(sys.argv[6])]   # grid steps in ulps per coefficient for this region
for it in range(12):
    r = means(C); base = match(C)
    H = sim_(C)
    A = np.zeros((nw, 18))
    for i in range(18):
        k = 4
        A[:, i] = sim.ulps(sim.b3(step(C, i, k * G[i])[:9], step(C, i, k * G[i])[9:], xx, mode), H).reshape(nw, 1024).mean(1) / k
    U, S, Vt = np.linalg.svd(A, full_matrices=False)
    best = (base, None)
    for kk in range(1, min(nw, 18) + 1):
        Sinv = np.where(np.arange(len(S)) < kk, 1 / S, 0)
        n = Vt.T @ (Sinv * (U.T @ r))
        for scale in (1.0, 0.5):
            nn = np.round(n * scale).astype(int)
            if not nn.any(): continue
            Cn = C.copy()
            for i in range(18): Cn = step(Cn, i, nn[i] * G[i])
            v = match(Cn)
            if v > best[0]: best = (v, Cn, kk, scale, nn)
    print(it, "match %.4f -> %.4f" % (base, best[0]), "" if best[1] is None else "rank %d scale %.1f n %s" % (best[2], best[3], best[4].tolist()), flush=True)
    if best[1] is None: break
    C = best[1]
    json.dump([sim.hx(v) for v in C], open(out, "w"))
