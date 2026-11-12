import numpy as np, sim, load, math
from fractions import Fraction as Fr
xs,E=load.load(); W=len(xs)//1024
X=xs.reshape(W,1024); EE=E.reshape(W,1024); cx=X[:,512]
R0=[w for w in range(W) if 2.47<cx[w]<2.53]
xx=np.ascontiguousarray(X[R0].ravel()); ee=EE[R0].ravel()
P,Q=sim.f(sim.B3_P).copy(),sim.f(sim.B3_Q).copy()
def m(P,Q,mode): return (sim.ulps(sim.b3(P,Q,xx,mode),ee)==0).mean()
print("H0 G0 %.3f G1 %.3f P-only %.3f"%(m(P,Q,0),m(P,Q,1),m(P,Q,4)))
gP=2.0**-40; gQ=2.0**-39
p0=round(P[0]/gP)*gP
best=[]
for k in range(-24,6):
    for mm in range(-8,14):
        Pn=P.copy(); Qn=Q.copy(); Pn[0]=p0+k*gP; Qn[0]=1.0+mm*gQ
        best.append((m(Pn,Qn,1),k,mm))
best.sort(reverse=True); print(best[:8])
print("Q0=1 exact rows:", [b for b in best if b[2]==0][:3])
