import ask, struct, collections, numpy as np
L = np.longdouble
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def hx(v): return struct.pack(">d", float(v)).hex()
rows = [l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex = [l.strip() for l in open("e2-excel.tsv")]
seed = {}
for r, e in zip(rows, ex):
    if r[1] == "1": seed[(r[0], r[2])] = fl(e)
keys = sorted(seed)
ys = [fl(k[1]) for k in keys]
res = {}
for K in (10, 11):
    got, meta = ask.ask_bits("GAMMA", [[hx(K + y)] for y in ys])
    res[K] = [fl("0x" + g["bits"]) for g in got]
c = collections.Counter()
for K in (10, 11):
    for i, (k, y) in enumerate(zip(keys, ys)):
        G = res[K][i]; s = seed[k]
        facs = [y + j for j in range(1, K)]       # 1+y .. (K-1)+y
        v = L(G)
        for f in reversed(facs): v = v / L(f)
        c[(K, "resident_desc", float(v) == s)] += 1
        v = L(G)
        for f in facs: v = v / L(f)
        c[(K, "resident_asc", float(v) == s)] += 1
        v = G
        for f in reversed(facs): v = float(L(v) / L(f))
        c[(K, "stored_desc", v == s)] += 1
        p = 1.0
        for f in facs: p = float(L(p) * L(f))
        c[(K, "product_then_div", float(L(G) / L(p)) == s)] += 1
        p = L(1)
        for f in facs: p = p * L(f)
        c[(K, "resident_product_div", float(L(G) / p) == s)] += 1
for k in sorted(c):
    if k[2]: print(k[0], k[1], c[k], "of", len(ys))
