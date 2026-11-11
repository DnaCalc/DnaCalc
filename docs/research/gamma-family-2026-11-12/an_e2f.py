import struct, collections, numpy as np, math
L=np.longdouble
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def hx(v): return "0x"+struct.pack(">d", float(v)).hex()
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
E={}
for r,e in zip(rows,ex): E[(r[0],int(r[1]),r[2])]=fl(e)
c=collections.Counter()
for (st,n,yh),e in E.items():
    if n>-1: continue
    y=fl(yh); x=n+y; s=E[(st,1,yh)]
    facs=[x+k for k in range(0,-n+1)]  # x .. y
    v=L(s)
    for f in facs: v=v/L(f)
    c[(st,n,"asc_res",float(v)==e)]+=1
    v=s
    for f in facs: v=float(L(v)/L(f))
    c[(st,n,"asc_st",v==e)]+=1
    p=L(1)
    for f in facs: p=p*L(f)
    c[(st,n,"prodasc_res",float(L(s)/p)==e)]+=1
    # stored product then resident? product stored each step ascending, final division
    p=1.0
    for f in facs: p=float(L(p)*L(f))
    c[(st,n,"prodasc_st",float(L(s)/L(p))==e)]+=1
    # reciprocal: s * (1/prod)
    p=L(1)
    for f in facs: p=p*L(f)
    c[(st,n,"recip",float(L(s)*(L(1)/p))==e)]+=1
keys=sorted({k[2] for k in c})
for st in ("q","h"):
    for n in range(-9,0): print(st,n," ".join("%s %d"%(k,c[(st,n,k,True)]) for k in keys))
