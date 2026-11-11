# The common scale of P and Q is invisible to the window means but not to the phases: test lambda.
import numpy as np, sim, load, json, sys, itertools, time
from fractions import Fraction as Fr
xs, E = load.load(); W = len(xs) // 1024
X = xs.reshape(W, 1024); EE = E.reshape(W, 1024); cx = X[:, 512]
mode = int(sys.argv[1])
fit_ws = [w for w in range(W) if abs(cx[w] - 2.5) < 0.0045]          # the windows at 2.5 used to fit phases
test_ws = [w for w in range(W) if 0.02 < abs(cx[w] - 2.5) < 0.034]   # held out
def arr(ws): return np.ascontiguousarray(X[ws].ravel()), EE[ws].ravel()
xf, ef = arr(fit_ws); xt, et = arr(test_ws)
def match(C, x, e): return (sim.b3(C[:9], C[9:], x, mode) == e).mean()
H = np.concatenate([sim.f(sim.B3_P), sim.f(sim.B3_Q)])
def scaled(lam):
    return np.array([float(Fr(lam) * Fr(c)) for c in H])
def step(C, i, m):
    Cn = C.copy(); Cn[i] = (np.array([C[i]]).view(np.int64) + int(m)).view(np.float64)[0]; return Cn
def local_fit(C, R=8, sweeps=6):
    best = match(C, xf, ef)
    for s in range(sweeps):
        imp = 0
        for i in range(18):
            for m in range(-R, R + 1):
                if m == 0: continue
                Cn = step(C, i, m); v = match(Cn, xf, ef)
                if v > best: best = v; C = Cn; imp += 1
        if not imp: break
    return C, best
lams = [Fr(1)]
q0 = Fr(H[9])
lams.append(1 / q0)                       # Q0 exactly 1
for k in (-4, -2, -1, 1, 2, 4):
    lams.append(Fr(1) + Fr(k, 10 ** 11))
for lam in lams:
    C = scaled(lam)
    t = time.time()
    m0 = (match(C, xf, ef), match(C, xt, et))
    C2, mf = local_fit(C)
    print("lambda-1 = %.3e  before fit %.3f/%.3f  after fit at 2.5 %.3f  held-out %.3f  Q0 %s  (%.0fs)" %
          (float(lam - 1), m0[0], m0[1], mf, match(C2, xt, et), sim.hx(C2[9]), time.time() - t), flush=True)
