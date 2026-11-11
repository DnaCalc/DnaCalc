import struct, math, collections, numpy as np
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def bits(v): return struct.unpack(">q", struct.pack(">d", v))[0]
rows=[l.rstrip("\n").split("\t") for l in open("e3-predictions.tsv")][1:]
ex=[l.strip() for l in open("e3-excel.tsv")]
ONE="0x3ff0000000000000"
C={};D={};Co={};Do={}
match=collections.Counter()
for r,e in zip(rows,ex):
    s,p,x,a,b,cum,o=r
    match[(s,o==e)]+=1
    if s=="a":
        key=(a,p)
        if cum==ONE: C[key]=fl(e); Co[key]=fl(o)
        else: D[key]=fl(e); Do[key]=fl(o)
print("ours vs Excel by set", dict(match))
eq=collections.Counter(); diffs=collections.defaultdict(dict)
for k in C:
    a,e=k; a0=round(fl(a)*4)/4
    ok=C[k]==math.ldexp(D[k],int(e)); eq[(a0,e,ok)]+=1
    diffs[a][e]=bits(C[k])-bits(math.ldexp(D[k],int(e)))
for k in sorted(eq): print(k, eq[k])
# pattern across exponents for 0.75
same=sum(1 for a,d in diffs.items() if round(fl(a)*4)/4==0.75 and len(set(d.values()))==1)
print("0.75 shapes with the same C-ldexp(D) at all three exponents:", same, "of", sum(1 for a in diffs if round(fl(a)*4)/4==0.75))
print("distribution of C-ldexp(D) at 0.75:", collections.Counter(v for a,d in diffs.items() if round(fl(a)*4)/4==0.75 for v in d.values()))
# ours vs Excel at 0.75, C and D separately
for name,X,Xo in (("C",C,Co),("D",D,Do)):
    cc=collections.Counter()
    for k in X:
        a0=round(fl(k[0])*4)/4; cc[(a0, X[k]==Xo[k])]+=1
    print(name, sorted(cc.items()))
