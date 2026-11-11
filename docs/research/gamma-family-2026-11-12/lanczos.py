# Seed family test: Lanczos forms of Gamma(1+y) against Excel's E5 answers.
# Coefficient sets are published facts: Lanczos (1964) gamma = 5, N = 6 in its commonly printed form;
# g = 7, n = 9 (the widely reproduced Godfrey set). sqrt(2 pi) as the binary64 constant.
import numpy as np, struct, math
L = np.longdouble
X = [l.split("\t") for l in open("e5-predictions.tsv").read().splitlines()[1:]]
E = [l.strip() for l in open("e5-excel.tsv")]
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
xs = np.array([fl(a) for a, _ in X])[::4]; ex = np.array([fl(e) for e in E])[::4]
y = xs - 1.0
SQ = float.fromhex("0x1.40d931ff62706p+1")   # 0x40040d931ff62706
def st(v): return np.asarray(v, dtype=L).astype(np.float64)
def Lr(v): return np.asarray(v, dtype=np.float64).astype(L)
sets = {
    "g5n6": (5.0, [1.000000000190015, 76.18009172947146, -86.50532032941677, 24.01409824083091,
                   -1.231739572450155, 0.1208650973866179e-2, -0.5395239384953e-5]),
    "g7n9": (7.0, [0.99999999999980993, 676.5203681218851, -1259.1392167224028, 771.32342877765313,
                   -176.61502916214059, 12.507343278686905, -0.13857109526572012, 9.9843695780195716e-6,
                   1.5056327351493116e-7]),
}
def lanczos_variants(g, c, z):
    out = {}
    # series A = c0 + sum c_k/(z+k)
    A = Lr(np.full(len(z), c[0]))
    As = np.full(len(z), c[0])
    for k in range(1, len(c)):
        A = A + L(c[k]) / (Lr(z) + L(k))
        As = st(Lr(As) + st(L(c[k]) / Lr(st(Lr(z) + L(k)))))
    t = st(Lr(z) + L(g + 0.5))
    lt = st(np.log(Lr(t)))
    zh = st(Lr(z) + L(0.5))
    for an, Av in (("A_res", A), ("A_st", Lr(As))):
        pw = st(np.exp(Lr(st(Lr(zh) * Lr(lt)))))
        et = st(np.exp(-Lr(t)))
        out[an + " sq*pw*et*A"] = st(Lr(st(Lr(st(L(SQ) * Lr(pw))) * Lr(et))) * Av)
        out[an + " sq*A*pw*et"] = st(Lr(st(Lr(st(L(SQ) * Av)) * Lr(pw))) * Lr(et))
        ex1 = st(np.exp(Lr(st(Lr(st(Lr(zh) * Lr(lt))) - Lr(t)))))
        out[an + " sq*exp(zh*lt-t)*A"] = st(Lr(st(L(SQ) * Lr(ex1))) * Av)
        out[an + " exp(...)*(sq*A)"] = st(Lr(ex1) * Lr(st(L(SQ) * Av)))
        # NR-like: exp(log(sq*A) + zh*lt - t)
        out[an + " exp(ln(sq*A)+zh*lt-t)"] = st(np.exp(Lr(st(Lr(st(np.log(Lr(st(L(SQ) * Av))))) + Lr(st(Lr(st(Lr(zh) * Lr(lt))) - Lr(t)))))))
        # split power t^(zh/2) twice
        ph = st(np.exp(Lr(st(Lr(st(Lr(zh) * L(0.5))) * Lr(lt)))))
        out[an + " sq*ph*(ph*et)*A"] = st(Lr(st(Lr(st(L(SQ) * Lr(ph))) * Lr(st(Lr(ph) * Lr(et))))) * Av)
    return out
for name, (g, c) in sets.items():
    for vn, v in lanczos_variants(g, c, y).items():
        d = v.view(np.int64) - ex.view(np.int64)
        print("%-6s %-28s match %.4f  mean %+.3f sd %.3f" % (name, vn, (d == 0).mean(), d.mean(), d.std()))
