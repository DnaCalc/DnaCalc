import numpy as np, pickle, sim, math, sys
A=np.load("stage_A.npy"); keys,r,se,cx=pickle.load(open("stage_meta.pkl","rb"))
P,Q=sim.f(sim.B3_P),sim.f(sim.B3_Q)
kk=int(sys.argv[1])
Aw=A/se[:,None]; rw=r/se
# scale columns to grid units so the min-norm solution is minimal in grid steps
g=[]
for (name,k,b) in keys:
    c=(P if name=="P" else Q)[k]
    g.append(np.spacing(abs(c)) if b==99 else (2.0**(b-52) if k>0 else 2.0**(b-63)))
g=np.array(g); As=Aw*g[None,:]
U,S,Vt=np.linalg.svd(As,full_matrices=False)
Sinv=np.where(np.arange(len(S))<kk,1/S,0); sol=Vt.T@(Sinv*(U.T@rw))
res=rw-As@sol
print("chi2/dof %.2f"%((res**2).sum()/(len(rw)-kk)))
for i,key in enumerate(keys): print(key, "%.2f"%sol[i])
