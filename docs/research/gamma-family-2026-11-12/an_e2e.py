import struct, collections, numpy as np, math
L=np.longdouble
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def hx(v): return "0x"+struct.pack(">d", float(v)).hex()
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
E={}
for r,e in zip(rows,ex): E[(r[0],int(r[1]),fl(r[2]))]=fl(e)
c=collections.Counter()
for (st,n,y),e in E.items():
    if st!="h" or n>-1: continue
    m=-(n+y); yy=1.0-y; k=-n-1
    if (st,k,yy) not in E: continue
    g=E[(st,k,yy)]   # Excel GAMMA(m)
    whole=math.floor(m); frac=m-whole; folded=1.0-frac if frac>0.5 else frac
    a=float(L(math.pi)*L(folded)); sine=float(np.sin(L(a)))
    z=float(L(sine)*L(m))
    v=float(L(math.pi)/(L(g)*L(z)))
    v=-v if whole%2==0 else v
    c[(n,"refl",hx(v)==hx(e))]+=1
    v2=float(L(math.pi)/L(float(L(g)*L(z)))); v2=-v2 if whole%2==0 else v2
    c[(n,"refl_st",hx(v2)==hx(e))]+=1
for n in range(-9,0): print(n, " ".join("%s %d/%d"%(k,c[(n,k,True)],c[(n,k,True)]+c[(n,k,False)]) for k in ("refl","refl_st")))
