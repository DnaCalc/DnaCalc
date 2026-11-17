import sys; sys.path.insert(0,'/home/ubuntu/claude-work/erf/pylib')
import mpmath as mp, numpy as np
mp.mp.dps=60
B=np.load('biasB_bins.npy'); tb,eb,sb=B[:,0],B[:,1],B[:,2]
ss=[mp.mpf(float(x))**2 for x in tb]; ts=[mp.mpf(float(x)) for x in tb]
ftrue=[mp.erfinv(t) for t in ts]
a,b=mp.mpf('0.5'),mp.mpf('0.73')
N=13; vs=[(a+b)/2-(b-a)/2*mp.cos(mp.pi*(k+mp.mpf(1)/2)/N) for k in range(N)]
G={'R':lambda v: mp.erfinv(mp.sqrt(v))/mp.sqrt(v),'y':lambda v: mp.erfinv(mp.sqrt(v))}
alphas=np.concatenate([-np.logspace(-18,-10,161),[0],np.logspace(-18,-10,161)])
out=[]
for kind,g in G.items():
    fv=[g(v) for v in vs]
    for m in range(2,11):
        n=12-m
        A=mp.matrix([[v**i for i in range(m+1)]+[-f*v**k for k in range(1,n+1)] for v,f in zip(vs,fv)])
        x0=mp.lu_solve(A,mp.matrix(fv)); U,S,V=mp.svd_r(A); vmin=V[V.rows-1,:]
        best=None
        for al in alphas:
            x=[mp.mpf(float(x0[i]+mp.mpf(al)*vmin[i])) for i in range(13)]
            P=x[:m+1]; Q=[mp.mpf(1)]+x[m+1:]
            e=[]
            for s,t,ft in zip(ss,ts,ftrue):
                pv=mp.mpf(0)
                for c in reversed(P): pv=pv*s+c
                qv=mp.mpf(0)
                for c in reversed(Q): qv=qv*s+c
                v=pv/qv*(t if kind=='R' else 1)
                e.append(float((v/ft-1)*2**53))
            e=np.array(e); chi=np.sum(((e-eb)/sb)**2)
            if best is None or chi<best[0]: best=(chi,al)
        out.append((best[0],kind,m,n,best[1])); print(kind,(m,n),'best chi %.0f at alpha %.3e'%best,flush=True)
