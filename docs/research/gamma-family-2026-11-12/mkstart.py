import numpy as np, sim, load, json, sys
xs,E=load.load(); W=len(xs)//1024
X=xs.reshape(W,1024); EE=E.reshape(W,1024); cx=X[:,512]
lo,hi,mode,out=float(sys.argv[1]),float(sys.argv[2]),int(sys.argv[3]),sys.argv[4]
R0=[w for w in range(W) if lo<cx[w]<hi]
xx=np.ascontiguousarray(X[R0].ravel()); ee=EE[R0].ravel()
P,Q=sim.f(sim.B3_P).copy(),sim.f(sim.B3_Q).copy()
def m(P,Q): return (sim.ulps(sim.b3(P,Q,xx,mode),ee)==0).mean()
best=(m(P,Q),P.copy(),Q.copy())
if mode&1:
    gP=2.0**-40; gQ=2.0**-39; p0=round(P[0]/gP)*gP
    for k in range(-24,8):
        for mm in range(-8,16):
            Pn=P.copy(); Qn=Q.copy(); Pn[0]=p0+k*gP; Qn[0]=1.0+mm*gQ; v=m(Pn,Qn)
            if v>best[0]: best=(v,Pn,Qn)
print("start match %.3f"%best[0])
json.dump([sim.hx(v) for v in np.concatenate([best[1],best[2]])],open(out,"w"))
