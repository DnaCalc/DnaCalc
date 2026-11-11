# Score candidate stored intermediates of the GAMMA seed by whether their rounding line appears in the
# per-window spectra of Excel's seed error (E5).
import numpy as np, math
spec = np.load("e5_spec.npy"); W = spec.shape[0]; fr = np.fft.rfftfreq(1024)
med = np.median(spec[:, 1:], axis=1)
ys = np.array([(w + 0.5) / 256 for w in range(W)])
LG = math.lgamma
def gam(y): return math.gamma(1 + y)
def ulp(v): return math.ulp(abs(v))
def fold(f):
    f = f % 1.0
    return min(f, 1 - f)
def present(w, f, tol=1.5):
    k = f * 1024; lo = int(math.floor(k - tol)); hi = int(math.ceil(k + tol))
    lo = max(lo, 1); hi = min(hi, 512)
    return spec[w, lo:hi + 1].max() > 8 * med[w]
def score(V, name, h=1e-7):
    hits = 0; n = 0
    for w, y in enumerate(ys):
        v = V(y)
        if not np.isfinite(v) or v == 0: continue
        d = (V(y + h) - V(y - h)) / (2 * h)
        f = fold(abs(d) * 2.0 ** -52 / ulp(v))
        if f < 0.004: continue
        n += 1; hits += present(w, f)
    return hits, n
cands = {}
cands["output Gamma(1+y)"] = gam
for g in (5.0, 6.024680040776729583740234375, 607 / 128, 7.0, 9.0):
    t = lambda y, g=g: y + g + 0.5
    cands["g=%.4f t" % g] = t
    cands["g=%.4f ln t" % g] = lambda y, t=t: math.log(t(y))
    cands["g=%.4f (y+.5) ln t" % g] = lambda y, t=t: (y + 0.5) * math.log(t(y))
    cands["g=%.4f (y+.5)ln t - t" % g] = lambda y, t=t: (y + 0.5) * math.log(t(y)) - t(y)
    cands["g=%.4f t^(y+.5)" % g] = lambda y, t=t: t(y) ** (y + 0.5)
    cands["g=%.4f t^((y+.5)/2)" % g] = lambda y, t=t: t(y) ** ((y + 0.5) / 2)
    cands["g=%.4f exp(-t)" % g] = lambda y, t=t: math.exp(-t(y))
    cands["g=%.4f pow*exp" % g] = lambda y, t=t: t(y) ** (y + 0.5) * math.exp(-t(y))
    cands["g=%.4f A" % g] = lambda y, t=t: gam(y) / (math.sqrt(2 * math.pi) * t(y) ** (y + 0.5) * math.exp(-t(y)))
    cands["g=%.4f sqrt2pi*A" % g] = lambda y, t=t: gam(y) / (t(y) ** (y + 0.5) * math.exp(-t(y)))
    cands["g=%.4f A*pow" % g] = lambda y, t=t: gam(y) / (math.sqrt(2 * math.pi) * math.exp(-t(y)))
    cands["g=%.4f A*exp" % g] = lambda y, t=t: gam(y) / (math.sqrt(2 * math.pi) * t(y) ** (y + 0.5))
    cands["g=%.4f sqrt2pi*pow" % g] = lambda y, t=t: math.sqrt(2 * math.pi) * t(y) ** (y + 0.5)
    cands["g=%.4f sqrt2pi*pow*exp" % g] = lambda y, t=t: math.sqrt(2 * math.pi) * t(y) ** (y + 0.5) * math.exp(-t(y))
for K in range(2, 12):
    x = lambda y, K=K: y + K
    cands["K=%d x" % K] = x
    cands["K=%d ln x" % K] = lambda y, x=x: math.log(x(y))
    cands["K=%d Gamma(x)" % K] = lambda y, x=x: math.gamma(x(y))
    cands["K=%d prod" % K] = lambda y, x=x, K=K: math.gamma(x(y)) / gam(y)
    cands["K=%d exp(-x)" % K] = lambda y, x=x: math.exp(-x(y))
    cands["K=%d x^x" % K] = lambda y, x=x: x(y) ** x(y)
    cands["K=%d x^(x-.5)" % K] = lambda y, x=x: x(y) ** (x(y) - 0.5)
    cands["K=%d 1/x" % K] = lambda y, x=x: 1 / x(y)
    cands["K=%d ln Gamma(x)" % K] = lambda y, x=x: math.lgamma(x(y))
out = []
for name, V in cands.items():
    h, n = score(V, name)
    out.append((h / max(n, 1), h, n, name))
out.sort(reverse=True)
for r in out[:45]: print("%.2f %3d/%3d  %s" % r)
