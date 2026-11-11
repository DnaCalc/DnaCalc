import numpy as np, sim, load, json, sys
xs, E = load.load(); W = len(xs) // 1024
X = xs.reshape(W, 1024); EE = E.reshape(W, 1024); cx = X[:, 512]
lo, hi, mode, src, out = float(sys.argv[1]), float(sys.argv[2]), int(sys.argv[3]), sys.argv[4], sys.argv[5]
R0 = [w for w in range(W) if lo < cx[w] < hi]
xx = np.ascontiguousarray(X[R0].ravel()); ee = EE[R0].ravel()
C = np.array([int(h, 16) for h in json.load(open(src))], dtype=np.uint64).view(np.float64).copy()
def score(C):
    d = sim.ulps(sim.b3(C[:9], C[9:], xx, mode), ee); return int((d == 0).sum())
def step(C, i, m):
    Cn = C.copy(); Cn[i] = (np.array([C[i]]).view(np.int64) + int(m)).view(np.float64)[0]; return Cn
best = score(C); print("start %.4f" % (best / len(xx)))
spans = {0: (8192, 8), 9: (32768, 32)}
for i in range(18):
    if i not in spans:
        spans[i] = (512, 1)
for it in range(4):
    for i in [0, 9] + [k for k in range(18) if k not in (0, 9)]:
        span, stp = spans[i]
        cands = [(score(step(C, i, m)), m) for m in range(-span, span + 1, stp)]
        s, m = max(cands)
        if s > best:
            best = s; C = step(C, i, m)
            print(it, "coef", i, "move", m, "%.4f" % (best / len(xx)), flush=True)
    json.dump([sim.hx(v) for v in C], open(out, "w"))
print("final %.4f" % (best / len(xx)))
