# Linear model of the window-mean residual on effective stage constants per (stage, binade).
import load, sim, numpy as np, math, collections
xs,E=load.load(); W=len(xs)//1024
P,Q=sim.f(sim.B3_P),sim.f(sim.B3_Q)
H=sim.b3(P,Q,xs,0); d=sim.ulps(E,H).astype(float)
r=d.reshape(W,1024).mean(1); se=d.reshape(W,1024).std(1)/32
cx=xs.reshape(W,1024)[:,512]
def poly(c,x): 
    v=c[8]
    for k in range(7,-1,-1): v=v*x+c[k]
    return v
def stagevals(c,x):
    v=c[8]; S={8:v}
    for k in range(7,-1,-1): v=v*x+c[k]; S[k]=v
    return S
cols=collections.OrderedDict()
rows=[]
for w in range(W):
    x=cx[w]; N=poly(P,x); D=poly(Q,x); y=(x-2)*N/D; uo=np.spacing(y)
    row={}
    for name,c,sgn in (("P",P,1),("Q",Q,-1)):
        S=stagevals(c,x)
        for k in range(0,9):
            g=(x-2)*x**k/D if name=="P" else -(x-2)*N*x**k/D**2
            b=math.floor(math.log2(abs(S[k]))) if k<8 else 99
            key=(name,k,b); row[key]=g/uo
            cols.setdefault(key,len(cols))
    rows.append(row)
A=np.zeros((W,len(cols)))
for w,row in enumerate(rows):
    for key,g in row.items(): A[w,cols[key]]=g
keys=list(cols)
print("unknowns",len(keys))
U,S,Vt=np.linalg.svd(A/se[:,None],full_matrices=False)
print("sv",np.array2string(S[:10],precision=2), "...", np.array2string(S[-8:],precision=3))
for kk in (10,15,20,25,30,35,len(keys)):
    Sinv=np.where(np.arange(len(S))<kk,1/S,0); sol=Vt.T@(Sinv*(U.T@(r/se)))
    res=r-A@sol; print(kk,"chi2/dof %.2f"%(((res/se)**2).sum()/(W-kk)), "rms %.3f"%np.sqrt((res**2).mean()))
np.save("stage_A.npy",A); 
import pickle; pickle.dump((keys,r,se,cx),open("stage_meta.pkl","wb"))
