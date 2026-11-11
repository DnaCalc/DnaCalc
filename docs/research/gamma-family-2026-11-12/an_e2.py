import struct, collections, numpy as np
from e2_chain import above, below
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def hx(v): return "0x"+struct.pack(">d", float(v)).hex()
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
seed={}
for r,e in zip(rows,ex):
    if int(r[1])==1: seed[(r[0],r[2])]=fl(e)
c=collections.Counter(); bad=collections.defaultdict(list)
for r,e in zip(rows,ex):
    st,n,y,x,ours=r; n=int(n); s=seed[(st,y)]; X=fl(x)
    c[(st,n,"ours",ours==e)]+=1
    if n>=2: c[(st,n,"above",hx(above(X,s))==e)]+=1
    if n<=0:
        R=hx(below(X,s,False))==e; S=hx(below(X,s,True))==e
        c[(st,n,"R",R)]+=1; c[(st,n,"S",S)]+=1
        if fl(y)==0.5: print("half n=%d x=%s excel %s R %s S %s ours %s"%(n,x,e,hx(below(X,s,False)),hx(below(X,s,True)),ours))
for st in ("q","h"):
    for n in range(-9,10):
        line=[st,n]
        for k in ("ours","above","R","S"):
            t=c[(st,n,k,True)]; f=c[(st,n,k,False)]
            if t+f: line.append("%s %d/%d"%(k,t,t+f))
        print(*line)
