# Pattern (high-passed) fit: for one coefficient at a time, find the move whose within-window pattern
# best explains the within-window residual pattern (Excel - model), ignoring window means.
import numpy as np, sim, load, json, sys, time
xs, E = load.load(); W = len(xs) // 1024
mode = int(sys.argv[1]); src = sys.argv[2]; out = sys.argv[3]
stride = int(sys.argv[4]) if len(sys.argv) > 4 else 4
ws = list(range(0, W, stride))
X = np.ascontiguousarray(xs.reshape(W, 1024)[ws].ravel()); Ex = E.reshape(W, 1024)[ws].ravel()
nw = len(ws)
C = np.array([int(h, 16) for h in json.load(open(src))], dtype=np.uint64).view(np.float64).copy()
def sim_(C): return sim.b3(C[:9], C[9:], X, mode)
def hp(d):
    d = d.reshape(nw, 1024).astype(float); return (d - d.mean(1, keepdims=True)).ravel()
def step(C, i, m):
    Cn = C.copy(); Cn[i] = (np.array([C[i]]).view(np.int64) + int(m)).view(np.float64)[0]; return Cn
spans = json.loads(sys.argv[5]) if len(sys.argv) > 5 else {}
t = time.time()
for sweep in range(int(sys.argv[6]) if len(sys.argv) > 6 else 3):
    H = sim_(C); r = hp(sim.ulps(Ex, H)); base = float(r @ r)
    print("sweep", sweep, "pattern ss %.0f  match %.4f" % (base, (sim.ulps(Ex, H) == 0).mean()), flush=True)
    for i in range(18):
        span, stp = spans.get(str(i), (64, 1))
        best = (base, 0)
        for m in range(-span, span + 1, stp):
            if m == 0: continue
            Hm = sim_(step(C, i, m)); dlt = hp(sim.ulps(Hm, H))
            res = r - dlt; v = float(res @ res)
            if v < best[0]: best = (v, m)
        if best[1]:
            C = step(C, i, best[1]); H = sim_(C); r = hp(sim.ulps(Ex, H)); base = float(r @ r)
            print("  coef", i, "move", best[1], "pattern ss %.0f match %.4f" % (base, (sim.ulps(Ex, H) == 0).mean()), "%.0fs" % (time.time() - t), flush=True)
    json.dump([sim.hx(v) for v in C], open(out, "w"))
