# Fine scan of a common scale of all 18 coefficients (relative changes of a few ulps).
import numpy as np, sim, load, json, sys
from fractions import Fraction as Fr
xs, E = load.load(); W = len(xs) // 1024
X = xs.reshape(W, 1024); EE = E.reshape(W, 1024); cx = X[:, 512]
mode = int(sys.argv[1]); src = sys.argv[2] if len(sys.argv) > 2 else None
if src: H = np.array([int(h, 16) for h in json.load(open(src))], dtype=np.uint64).view(np.float64)
else: H = np.concatenate([sim.f(sim.B3_P), sim.f(sim.B3_Q)])
groups = {"at2.5": [w for w in range(W) if abs(cx[w] - 2.5) < 0.0045],
          "near": [w for w in range(W) if 0.01 < abs(cx[w] - 2.5) < 0.034],
          "all": list(range(0, W, 2))}
data = {k: (np.ascontiguousarray(X[v].ravel()), EE[v].ravel()) for k, v in groups.items()}
def match(C, k): x, e = data[k]; return (sim.b3(C[:9], C[9:], x, mode) == e).mean()
best = None
for j in range(-40, 41):
    lam = Fr(1) + Fr(j, 5 * 10 ** 16)
    C = np.array([float(lam * Fr(c)) for c in H])
    r = (match(C, "at2.5"), match(C, "near"), match(C, "all"))
    if j % 4 == 0 or (best and r[1] > best[1][1]): print("lambda-1 %+.2e  at2.5 %.3f  near %.3f  all %.3f" % (float(lam - 1), *r), flush=True)
    if best is None or r[1] > best[1][1]: best = (j, r)
print("best", best)
