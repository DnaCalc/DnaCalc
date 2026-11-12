# Per-window spectra of Excel's seed error (Excel - truth, in output ulps), E5 windows.
import numpy as np, struct, hpgamma, math, sys
from decimal import Decimal as D
from fractions import Fraction as Fr
X = [l.split("\t") for l in open("e5-predictions.tsv").read().splitlines()[1:]]
E = [l.strip() for l in open("e5-excel.tsv")]
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
xs = np.array([fl(a) for a, _ in X]); ex = np.array([fl(e) for e in E])
W = 256
def dec(v): f = Fr(v); return D(f.numerator) / D(f.denominator)
errs = np.zeros((W, 1024))
for w in range(W):
    seg = slice(w * 1024, (w + 1) * 1024)
    x0 = xs[w * 1024]; x1 = xs[w * 1024 + 1023]
    g0 = hpgamma.gamma_hp(x0); g1 = hpgamma.gamma_hp(x1)
    slope = (g1 - g0) / (dec(x1) - dec(x0))
    u = dec(math.ulp(ex[w * 1024 + 512]))
    # x_j = x0 + j*2^-52 exactly; truth_j = g0 + j*2^-52*slope
    step = slope * dec(2.0 ** -52)
    e0 = [(dec(ex[w * 1024 + j]) - (g0 + j * step)) / u for j in range(1024)]
    errs[w] = np.array([float(v) for v in e0])
np.save("e5_errs.npy", errs)
spec = np.abs(np.fft.rfft(errs - errs.mean(1, keepdims=True), axis=1)) ** 2
np.save("e5_spec.npy", spec)
freqs = np.fft.rfftfreq(1024)
for w in range(0, W, 8):
    top = np.argsort(spec[w])[::-1][:4]
    print("y %.4f " % (xs[w * 1024 + 512] - 1) + "  ".join("f %.4f p %.0f" % (freqs[k], spec[w, k]) for k in top))
