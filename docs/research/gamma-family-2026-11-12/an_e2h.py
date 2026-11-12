import struct, collections, numpy as np
L=np.longdouble
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
E={}
for r,e in zip(rows,ex): E[(r[0],int(r[1]),fl(r[2]))]=fl(e)
c=collections.Counter()
for (st,n,y),e in E.items():
    if n>0: continue
    x=n+y
    for k in range(1,10):
        s=E[(st,k,y)]; v=L(s); j=k-1-n
        while j>=0: v=v/L(x+j); j-=1
        c[(st,n,k,float(v)==e)]+=1
for st in ("q","h"):
    for n in range(-9,1): print(st,n," ".join("k%d %d"%(k,c[(st,n,k,True)]) for k in range(1,10)))
