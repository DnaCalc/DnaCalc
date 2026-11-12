import struct, collections, numpy as np
L=np.longdouble
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def bits(v): return struct.unpack(">q", struct.pack(">d", v))[0]
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
E={}
for r,e in zip(rows,ex): E[(r[0],int(r[1]),fl(r[2]))]=fl(e)
for st in ("q","h"):
  for n in (-1,-2):
    d=[]
    for (s_,n_,y),e in sorted(E.items()):
        if s_!=st or n_!=n: continue
        x=n+y; s=E[(st,1,y)]; v=L(s); j=-n
        while j>=0: v=v/L(x+j); j-=1
        d.append(bits(e)-bits(float(v)))
    d=np.array(d); print(st,n,collections.Counter(d.tolist()), "run:", "".join("0" if v==0 else ("+" if v>0 else "-") for v in d[:120]))
