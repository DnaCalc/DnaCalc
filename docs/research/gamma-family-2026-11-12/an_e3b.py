import struct, math, collections, numpy as np
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def bits(v): return struct.unpack(">q", struct.pack(">d", v))[0]
rows=[l.rstrip("\n").split("\t") for l in open("e3-predictions.tsv")][1:]
ex=[l.strip() for l in open("e3-excel.tsv")]
ONE="0x3ff0000000000000"
seam=collections.defaultdict(list); S=collections.defaultdict(dict)
for r,e in zip(rows,ex):
    s,p,x,a,b,cum,o=r
    if s=="m":
        X=fl(x); side="below" if X<1.1 else "above"
        seam[(fl(a),cum==ONE,side)].append(bits(fl(o))-bits(fl(e)))
    if s=="s":
        S[(a,x if float(p)==1.0 else None)]
        S[a+"|"+str(round(fl(x)/float(p),300))][float(p)]=fl(e)
for k in sorted(seam):
    d=np.array(seam[k]); print(k, "n",len(d),"mean(ours-Excel) %.2f"%d.mean(), "neg %d zero %d pos %d"%((d<0).sum(),(d==0).sum(),(d>0).sum()))
ok=0; tot=0
for key,d in S.items():
    if "|" not in key or 1.0 not in d: continue
    tot+=1; ok+= all(d[b]==d[1.0]/b for b in d)
print("scale-exact shapes (Excel):", ok, "of", tot)
