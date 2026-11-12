import struct, collections, numpy as np
L=np.longdouble
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def hx(v): return "0x"+struct.pack(">d", float(v)).hex()
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
E={}
for r,e in zip(rows,ex): E[(r[0],int(r[1]),r[2])]=fl(e)
c=collections.Counter()
for (st,n,yh),e in E.items():
    if n>0: continue
    y=fl(yh); x=n+y
    for k in (1,2,3):
        s=E[(st,k,yh)]
        # divide by (x+K-1) ... down to x, resident; K = k - n
        v=L(s); j=k-1-n
        while j>=0: v=v/L(x+j); j-=1
        c[(st,n,"seed%d_res"%k, hx(float(v))==hx(e))]+=1
        v=s; j=k-1-n
        while j>=0: v=float(L(v)/L(x+j)); j-=1
        c[(st,n,"seed%d_st"%k, hx(v)==hx(e))]+=1
    # chain from the previous Excel answer: GAMMA(x) = st(GAMMA(x+1)/x) using Excel GAMMA(x+1)
    prev=E[(st,n+1,yh)]
    c[(st,n,"prevExcel_div", hx(float(L(prev)/L(x)))==hx(e))]+=1
keys=sorted({k[2] for k in c})
for st in ("q","h"):
    for n in range(-9,1):
        print(st,n," ".join("%s %d"%(k,c[(st,n,k,True)]) for k in keys))
