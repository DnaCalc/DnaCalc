import load, sim, numpy as np
xs,E=load.load()
W=len(xs)//1024
P0,Q0=sim.f(sim.B3_P),sim.f(sim.B3_Q)
C0=np.concatenate([P0,Q0])
def ev(C,mode=0): return sim.b3(C[:9],C[9:],xs,mode)
def bump(C,k,s):
    b=C.view(np.int64).copy(); b[k]+=s; return b.view(np.float64)
def wmean(d): return d.reshape(W,1024).mean(1)
H=ev(C0); d0=sim.ulps(E,H).astype(float); r=wmean(d0)
J=np.zeros((W,18))
s=32
for k in range(18):
    Hk=ev(bump(C0,k,s)); J[:,k]=wmean(sim.ulps(Hk,H).astype(float))/s
np.save("J.npy",J); np.save("r.npy",r)
U,S,Vt=np.linalg.svd(J,full_matrices=False)
print("sv",np.array2string(S,precision=3))
print("resid rms before",np.sqrt((r**2).mean()))
for kk in range(4,19):
    Sinv=np.where(np.arange(18)<kk,1/S,0)
    dl=Vt.T@(Sinv*(U.T@r))
    res=r-J@dl
    print(kk, "rms",np.sqrt((res**2).mean()).round(4), "maxstep",np.abs(dl).max().round(1))
