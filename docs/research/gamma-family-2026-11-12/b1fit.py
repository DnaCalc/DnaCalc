# B1 band [0.7, 1.5): (x - 1) * (N/D) in plain binary64, Horner descending over ascending coefficients
# (the landed stand-in's graph). Fit the 16 coefficients to Excel's E7 answers with compensated pairs.
import numpy as np, json, sys, itertools, time, struct
P1 = [0xC001ABB666D0134A, 0xC0535A47EFFC8A03, 0xC0802D1B5724230B, 0xC08CD2174AAA2285,
      0xC0705B8CBB60E8EC, 0x406E659B984EC72C, 0x40556C26056BAD85, 0x40107BBE50ECA47A]
Q1 = [0x3FDA39D2EC8BFCDF, 0x40386F690BC7F9C5, 0x407064EEFD4AC8D5, 0x408A709AB2B96CB8,
      0x408DBA96BA615B30, 0x40779D655EAA2824, 0x4046D2C96BBB64C7, 0x3FF0000000000000]
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
rows = [l.split("\t") for l in open("e7-predictions.tsv").read().splitlines()[1:]]
X = np.array([int(r[0], 16) for r in rows], dtype=np.uint64).view(np.float64)
OURS = np.array([int(r[1], 16) for r in rows], dtype=np.uint64).view(np.float64)
EX = np.array([int(l, 16) for l in open("e7-excel.tsv").read().split()], dtype=np.uint64).view(np.float64)
def b1(C, x):
    n = np.zeros_like(x); d = np.zeros_like(x)
    for c in C[7::-1]: n = n * x + c
    for c in C[15:7:-1]: d = d * x + c
    return (x - 1.0) * (n / d)
C0 = np.concatenate([np.array(P1, dtype=np.uint64).view(np.float64), np.array(Q1, dtype=np.uint64).view(np.float64)])
assert (b1(C0, X) == OURS).all(), "model differs from calc-func"
mode = sys.argv[1] if len(sys.argv) > 1 else "fit"
per = 64
sel = np.concatenate([np.arange(w * 1024, w * 1024 + 1024, 1024 // per) for w in range(256)])
xx = X[sel]; ee = EX[sel]
def score(C): return int((b1(C, xx) == ee).sum())
def step(C, i, m):
    Cn = C.copy(); Cn[i] = (np.array([C[i]]).view(np.int64) + int(m)).view(np.float64)[0]; return Cn
def gains(C, x0):
    N = 0.0; D = 0.0
    for c in C[7::-1]: N = N * x0 + c
    for c in C[15:7:-1]: D = D * x0 + c
    return np.array([((x0 - 1) * x0 ** (i % 8) * np.spacing(abs(C[i])) / D) if i < 8 else
                     (-(x0 - 1) * N * x0 ** (i % 8) * np.spacing(abs(C[i])) / D ** 2) for i in range(16)])
C = C0.copy()
if len(sys.argv) > 2: C = np.array([int(h, 16) for h in json.load(open(sys.argv[2]))], dtype=np.uint64).view(np.float64).copy()
best = score(C); n = len(xx); print("start %.4f" % (best / n), flush=True); t = time.time()
for sweep in range(8):
    imp = 0
    for x0 in (1.1, 0.8, 1.4, 0.95, 1.25):
        # singles
        for i in range(16):
            for m in (1, -1, 2, -2, 4, -4, 8, -8, 16, -16, 64, -64, 256, -256):
                Cn = step(C, i, m); s = score(Cn)
                if s > best: best = s; C = Cn; imp += 1
        g = gains(C, x0)
        for i, j in itertools.permutations(range(16), 2):
            for mi in (1, 2, 4, 8, 16, 32, 64, 128, 512, 2048):
                for sgn in (1, -1):
                    m = sgn * mi; mj0 = -m * g[i] / g[j]
                    if abs(mj0) > 1e7: continue
                    for e in (-1, 0, 1):
                        Cn = step(step(C, i, m), j, int(round(mj0)) + e); s = score(Cn)
                        if s > best: best = s; C = Cn; imp += 1
        print(sweep, "x0", x0, "%.4f" % (best / n), "%.0fs" % (time.time() - t), flush=True)
        json.dump(["0x" + struct.pack(">d", v).hex() for v in C], open("b1fit.json", "w"))
    if not imp: break
full = (b1(C, X) == EX).mean(); print("final sample %.4f full %.4f" % (best / n, full))
