import ask, struct, collections
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def hx(v): return struct.pack(">d", float(v)).hex()
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
T=[(r[3],e) for r,e in zip(rows,ex) if r[1]=="1"]
g,_=ask.ask_bits("GAMMALN",[[t[2:]] for t,_ in T])
e,_=ask.ask_bits("EXP",[[r["bits"]] for r in g])
c=collections.Counter(); d=collections.Counter()
for (t,G),r in zip(T,e):
    ok=("0x"+r["bits"])==G; c[ok]+=1
    d[struct.unpack(">q",bytes.fromhex(r["bits"]))[0]-struct.unpack(">q",bytes.fromhex(G[2:]))[0]]+=1
print(c, sorted(d.items()))
