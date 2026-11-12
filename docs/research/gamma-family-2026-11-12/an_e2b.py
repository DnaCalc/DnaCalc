import struct, collections, numpy as np, math
from e2_chain import below
L=np.longdouble
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def hx(v): return "0x"+struct.pack(">d", float(v)).hex()
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
E={}
for r,e in zip(rows,ex): E[(r[0],int(r[1]),r[2])]=fl(e)
def cands(y,n,s):
    x=n+y; facs=[x+k for k in range(0,-n+1)]   # x, x+1, ..., x-n = y  (k from 0 to -n)
    out={}
    # product from y down to x (y, y-1, ...), stored each
    p=1.0
    for f in reversed(facs): p=float(L(p)*L(f))
    out["prod_down_st"]=float(L(s)/L(p))
    p=1.0
    for f in facs: p=float(L(p)*L(f))
    out["prod_up_st"]=float(L(s)/L(p))
    p=L(1.0)
    for f in reversed(facs): p=p*L(f)
    out["prod_down_res"]=float(L(s)/p)
    out["R"]=below(x,s,False); out["S"]=below(x,s,True)
    return out
c=collections.Counter()
for (st,n,y),e in E.items():
    if n>0: continue
    s=E[(st,1,y)]
    for k,v in cands(fl(y),n,s).items(): c[(st,n,k,hx(v)==hx(e))]+=1
keys=["R","S","prod_down_st","prod_up_st","prod_down_res"]
for st in ("q","h"):
    for n in range(-9,1):
        print(st,n," ".join("%s %d"%(k,c[(st,n,k,True)]) for k in keys))
