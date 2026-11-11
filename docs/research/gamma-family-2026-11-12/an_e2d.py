# Common resident seed: is there one 64-bit seed per y that rounds to Excel GAMMA(1+y) and gives every n<=0 row?
import struct, collections, numpy as np, math, sys
L=np.longdouble
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
E={}
for r,e in zip(rows,ex): E[(r[0],int(r[1]),r[2])]=fl(e)
ys=sorted({(r[0],r[2]) for r in rows})
stats=collections.Counter(); offs=[]
for st,yh in ys:
    y=fl(yh); s=E[(st,1,yh)]
    e=math.frexp(s)[1]-1
    k=np.arange(-1024,1025,dtype=np.int64)
    s64=L(s)+k.astype(L)*L(2.0)**(e-63)
    ok=(s64.astype(np.float64)==s)
    good=np.ones(len(k),bool)&ok
    nmatch=np.zeros(len(k),int)
    for n in range(0,-10,-1):
        x=n+y; v=s64.copy(); j=-n
        while j>=0: v=v/L(x+j); j-=1
        m=(v.astype(np.float64)==E[(st,n,yh)])
        nmatch+=m; good&=m
    best=nmatch[ok].max()
    stats[(st,best)]+=1
    if good.any(): offs.append((st,y,k[good].min(),k[good].max()))
for kk in sorted(stats): print(kk, stats[kk])
print("y with a consistent resident seed:", len(offs), "of", len(ys))
for o in offs[:10]: print(o)
