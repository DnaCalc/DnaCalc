import numpy as np, pickle, sim, math
A=np.load("stage_A.npy"); keys,r,se,cx=pickle.load(open("stage_meta.pkl","rb"))
P,Q=sim.f(sim.B3_P),sim.f(sim.B3_Q)
Aw=A/se[:,None]; rw=r/se
# full LS with covariance
U,S,Vt=np.linalg.svd(Aw,full_matrices=False)
sol=Vt.T@((U.T@rw)/S); cov=Vt.T@np.diag(1/S**2)@Vt; sd=np.sqrt(np.diag(cov))
for i,(name,k,b) in enumerate(keys):
    c=(P if name=="P" else Q)[k]
    if b==99: g=np.spacing(abs(c))
    else: g=2.0**(b-52) if k>0 else 2.0**(b-63)
    print(name,k,b, "delta/grid %9.3f  sd %8.3f"%(sol[i]/g, sd[i]/g), " ulp(c)/grid %.4g"%(np.spacing(abs(c))/g))
