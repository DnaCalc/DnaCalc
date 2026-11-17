import numpy as np, itertools
B=np.load('biasB_bins.npy'); tb,eb,sb=B[:,0],B[:,1],B[:,2]
s=tb**2
def test(a,b,N,var='s'):
    v = s if var=='s' else tb
    nodes=(a+b)/2-(b-a)/2*np.cos(np.pi*(np.arange(N)+0.5)/N)
    w=np.prod(v[:,None]-nodes[None,:],axis=1)
    w=w/np.abs(w).max()
    cols=[np.ones_like(tb),tb-0.78,(tb-0.78)**2]+[w*(tb-0.78)**k for k in range(3)]
    X=np.vstack(cols).T
    coef,*_=np.linalg.lstsq(X/sb[:,None],eb/sb,rcond=None)
    return np.sum(((X@coef-eb)/sb)**2)
best=[]
for var in ['s','t']:
    for N in range(10,17):
        for a in np.linspace(0.46,0.52,25) if var=='s' else np.linspace(0.68,0.72,25):
            for b in np.linspace(0.70,0.78,33) if var=='s' else np.linspace(0.84,0.89,26):
                best.append((test(a,b,N,var),var,N,round(a,4),round(b,4)))
best.sort()
for r in best[:12]: print(r)
