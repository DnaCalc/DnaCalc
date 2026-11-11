import numpy as np, pickle, sim, math, sys
from surrogate import Surrogate
keys, r, se, cx = pickle.load(open("stage_meta.pkl", "rb"))
P, Q = sim.f(sim.B3_P), sim.f(sim.B3_Q)
import os
LS=tuple(bool(int(v)) for v in os.environ.get("LS","00"))
S = Surrogate(cx, r, se, P, Q, LS)
C0 = np.concatenate([P, Q]); W = len(cx)
U = np.array([np.spacing(abs(c)) for c in C0])
# variables: delta_i (18, in ulps of c_i), eta per distinct (i, grid tuple) in absolute units
cols = {}
for w in range(W):
    for i in range(18):
        g = S.grid[w][i]
        if g is None: continue
        cols.setdefault((i, g), len(cols))
nv = 18 + len(cols)
A = np.zeros((W, nv)); prior_sd = np.zeros(nv)
for w in range(W):
    for i in range(18):
        A[w, i] = S.gain[w, i] * U[i]
        g = S.grid[w][i]
        if g is not None: A[w, 18 + cols[(i, g)]] = S.gain[w, i] * g[0]
for (i, g), j in cols.items():
    gs, gp = g
    prior_sd[18 + j] = (1 / 3) if gs > U[i] else 1e-6
prior_sd[:18] = float(sys.argv[1]) if len(sys.argv) > 1 else 1e6
Aw = A / se[:, None]; rw = r / se
Lam = np.diag(1 / prior_sd**2)
M = Aw.T @ Aw + Lam
cov = np.linalg.inv(M); mu = cov @ (Aw.T @ rw)
res = rw - Aw @ mu; print("chi2 %.1f" % (res @ res), "W", W, "vars", nv)
for i in range(18):
    print(i, "delta %12.1f ulps  sd %12.1f" % (mu[i], math.sqrt(cov[i, i])))
for (i, g), j in sorted(cols.items()):
    v = mu[18 + j]
    print("  eta", i, "g/ulp=%.3g" % (g[0] / U[i]), "gp/gs=%.3g" % (g[1] / g[0]),
          "eta/g %.3f sd %.3f" % (v, math.sqrt(cov[18 + j, 18 + j])))
if len(sys.argv)>2:
    rr=res*se
    for w in range(0,W):
        if abs(res[w])>4: print("w %.4f r %.3f resid %.3f"%(cx[w], r[w], rr[w]))
