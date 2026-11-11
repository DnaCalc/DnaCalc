# Separate the coefficient table into residues (which fix the hidden rounding offsets at each stage grid)
# and a smooth part (which the window means fit by least squares). Coordinate descent on the residues,
# with the smooth part projected out at every step.
import numpy as np, pickle, sim, json, os, sys, math, time
from surrogate import Surrogate
META = os.environ["META"]; LS = tuple(bool(int(v)) for v in os.environ["LS"]); OUT = os.environ["OUT"]
_, r, se, cx = pickle.load(open(META, "rb"))
P, Q = sim.f(sim.B3_P), sim.f(sim.B3_Q)
S = Surrogate(cx, r, se, P, Q, LS)
C0 = np.concatenate([P, Q]); W = len(cx)
U = np.array([np.spacing(abs(c)) for c in C0])
# period of the offset pattern per coefficient: largest stage grid over ulp
m = []
for i in range(18):
    gs = [S.grid[w][i][0] for w in range(W) if S.grid[w][i] is not None]
    m.append(max(1, int(round(max(gs) / U[i]))) if gs else 1)
m = np.array(m); print("periods", m.tolist())
def step(c, n): return (np.array([c]).view(np.int64) + int(n)).view(np.float64)[0]
def offset(i, rho):
    """per-window offset pattern (chi units) of coefficient i at residue rho (in ulps), smooth part removed"""
    c = step(C0[i], rho)
    return S.gain[:, i] * (S.dC(i, c) - rho * U[i]) / se
G = (S.gain * U[None, :]) / se[:, None]          # smooth columns, chi units per ulp
rw = r / se
lam = float(os.environ.get("RIDGE", "1e-8"))
Gt = G.T @ G + lam * np.eye(18)
Proj = np.eye(W) - G @ np.linalg.solve(Gt, G.T)
rho = np.zeros(18, dtype=np.int64)
cache = {}
def off(i, v):
    k = (i, int(v))
    if k not in cache: cache[k] = offset(i, int(v))
    return cache[k]
def obj(rho):
    tot = rw.copy()
    for i in range(18):
        if m[i] > 1: tot = tot - off(i, rho[i])
    pr = Proj @ tot
    return float(pr @ pr)
best = obj(rho); print("start projected chi2 %.1f" % best); t = time.time()
for sweep in range(20):
    imp = 0
    for i in range(18):
        if m[i] == 1: continue
        for v in range(m[i]):
            if v == rho[i]: continue
            rn = rho.copy(); rn[i] = v; o = obj(rn)
            if o < best - 1e-9: best = o; rho = rn; imp += 1
    print(sweep, "projected chi2 %.1f" % best, rho.tolist(), "%.0fs" % (time.time() - t), flush=True)
    if not imp: break
# smooth part given residues: continuous LS, then round to the residue class
tot = rw.copy()
for i in range(18):
    if m[i] > 1: tot = tot - off(i, rho[i])
n_cont = np.linalg.solve(Gt, G.T @ tot)
n = np.array([rho[i] + m[i] * round((n_cont[i] - rho[i]) / m[i]) for i in range(18)], dtype=np.int64)
C = np.array([step(C0[i], n[i]) for i in range(18)])
print("n (ulps)", n.tolist()); print("surrogate chi2 %.1f" % S.chi2(C))
json.dump([sim.hx(v) for v in C], open(OUT, "w"))
