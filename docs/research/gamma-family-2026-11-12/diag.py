import numpy as np, sim, load, json, sys
L=np.longdouble
xs,E=load.load(); W=len(xs)//1024
X=xs.reshape(W,1024); EE=E.reshape(W,1024); cx=X[:,512]
lo,hi=float(sys.argv[1]),float(sys.argv[2]); mode=int(sys.argv[3])
C=np.array([int(h,16) for h in json.load(open(sys.argv[4]))],dtype=np.uint64).view(np.float64)
R0=[w for w in range(W) if lo<cx[w]<hi]
x=np.ascontiguousarray(X[R0].ravel()); e=EE[R0].ravel()
fr={}
def st(v,name):
    d=v.astype(np.float64); u=np.spacing(np.abs(d)); fr[name]=((v-d.astype(L))/u.astype(L)).astype(np.float64); return d
def chain(c,tag,store_last):
    v=np.full(len(x),c[8])
    for k in range(7,0,-1):
        p=st(L(1)*v.astype(L)*x.astype(L),tag+"p%d"%(k+1)); v=st(p.astype(L)+L(c[k]),tag+"s%d"%k)
    if mode&48: s=v.astype(L)*x.astype(L)+L(c[0])
    else:
        p=st(v.astype(L)*x.astype(L),tag+"p1"); s=p.astype(L)+L(c[0])
    return st(s,tag+"s0").astype(L) if store_last else s
N=chain(C[:9],"P",bool(mode&5)); D=chain(C[9:],"Q",bool(mode&9))
r=st(N/D,"R"); y=st(r.astype(L)*(x-2.0).astype(L),"y")
assert (y==sim.b3(C[:9],C[9:],x,mode)).all()
bad=y!=e
print("rows",len(x),"mismatch",bad.sum())
for k,f in fr.items():
    a=np.abs(f); 
    mb,mg=(a[bad]>0.45).mean(),(a[~bad]>0.45).mean()
    if abs(mb-mg)>0.03: print("%5s  near-tie share among mismatches %.3f  among matches %.3f"%(k,mb,mg))
