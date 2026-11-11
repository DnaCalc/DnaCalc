import struct, collections, numpy as np, math
L=np.longdouble
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
E={}
for r,e in zip(rows,ex): E[(r[0],int(r[1]),fl(r[2]))]=fl(e)
PI=math.pi
c=collections.Counter()
for (st,n,y),e in E.items():
    if st!="h" or n>-1: continue
    x=n+y; yy=1.0-y; k=-n
    if (st,k,yy) not in E: continue
    G=E[(st,k,yy)]          # Excel GAMMA(1-x)
    for sv,sname in ((float(np.sin(L(float(L(PI)*L(x))))),"sin_st_arg"),(float(np.sin(L(PI)*L(x))),"sin_res_arg")):
        c[(n,sname+"_res",float(L(PI)/(L(sv)*L(G)))==e)]+=1
        c[(n,sname+"_st",float(L(PI)/L(float(L(sv)*L(G))))==e)]+=1
        c[(n,sname+"_div2",float(L(float(L(PI)/L(sv)))/L(G))==e)]+=1
keys=sorted({k[1] for k in c})
for n in range(-9,0): print(n," ".join("%s %d"%(k,c[(n,k,True)]) for k in keys))
