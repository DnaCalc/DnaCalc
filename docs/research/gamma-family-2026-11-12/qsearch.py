import numpy as np, json, os, sys, itertools, time
exec(open("residues.py").read().split("best = obj(rho)")[0])
rho=np.array(json.loads(os.environ["RHO"]),dtype=np.int64)
print("projected chi2 at rho %.1f"%obj(rho))
tot=rw.copy()
for i in range(18):
    if m[i]>1: tot=tot-off(i,rho[i])
def build(q):
    n=rho+m*q; return np.array([step(C0[i],n[i]) for i in range(18)]), n
for lamv in (1e-4,1e-2,1,100):
    n_cont=np.linalg.solve(G.T@G+lamv*np.eye(18),G.T@tot)
    q=np.round((n_cont-rho)/m).astype(np.int64)
    C,n=build(q); print("lam",lamv,"chi2 %.1f"%S.chi2(C), "max|n| %d"%np.abs(n).max())
lamv=float(os.environ.get("LAM","1"))
n_cont=np.linalg.solve(G.T@G+lamv*np.eye(18),G.T@tot); q=np.round((n_cont-rho)/m).astype(np.int64)
C,n=build(q); best=S.chi2(C); t=time.time()
for sweep in range(60):
    imp=0
    for i in range(18):
        for d in (-3,-2,-1,1,2,3):
            qn=q.copy(); qn[i]+=d; Cn,_=build(qn); v=S.chi2(Cn)
            if v<best-1e-6: best=v; q=qn; imp+=1
    for i,j in itertools.combinations(range(18),2):
        for di in (-1,1):
            for dj in (-2,-1,1,2):
                qn=q.copy(); qn[i]+=di; qn[j]+=dj; Cn,_=build(qn); v=S.chi2(Cn)
                if v<best-1e-6: best=v; q=qn; imp+=1
    C,n=build(q); print(sweep,"chi2 %.1f"%best,"n",n.tolist(),"%.0fs"%(time.time()-t),flush=True)
    json.dump([sim.hx(v) for v in C],open(OUT,"w"))
    if not imp: break
