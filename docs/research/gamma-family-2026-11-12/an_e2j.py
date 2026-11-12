import struct, collections, numpy as np
L=np.longdouble
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
rows=[l.rstrip("\n").split("\t") for l in open("e2-predictions.tsv")][1:]
ex=[l.strip() for l in open("e2-excel.tsv")]
E={}
for r,e in zip(rows,ex): E[(r[0],int(r[1]),fl(r[2]))]=fl(e)
st=lambda v: L(float(v))
forms={
 "s/y/x":lambda s,y,x: (L(s)/L(y))/L(x),
 "s/(yx)":lambda s,y,x: L(s)/(L(y)*L(x)),
 "s/st(yx)":lambda s,y,x: L(s)/st(L(y)*L(x)),
 "s*st(1/y)*st(1/x)":lambda s,y,x: L(s)*st(L(1)/L(y))*st(L(1)/L(x)),
 "s*st(1/st(yx))":lambda s,y,x: L(s)*st(L(1)/st(L(y)*L(x))),
 "s*(1/y)*(1/x)":lambda s,y,x: L(s)*(L(1)/L(y))*(L(1)/L(x)),
 "st(s/x)/y":lambda s,y,x: st(L(s)/L(x))/L(y),
 "(s/x)/y":lambda s,y,x: (L(s)/L(x))/L(y),
 "s/(x*y) with x=st(y-1)":lambda s,y,x: L(s)/(L(y)*L(x)),
 "s/(y*y - y)":lambda s,y,x: L(s)/(L(y)*L(y)-L(y)),
 "s/st(st(y*y)-y)":lambda s,y,x: L(s)/st(st(L(y)*L(y))-L(y)),
 "s/(st(y*y)-y)":lambda s,y,x: L(s)/(st(L(y)*L(y))-L(y)),
}
for sname in ("q","h"):
    c=collections.Counter()
    for (s_,n,y),e in E.items():
        if s_!=sname or n!=-1: continue
        s=E[(s_,1,y)]; x=y-1.0
        for k,f in forms.items(): c[k]+= float(f(s,y,x))==e
    print(sname, dict(c))
