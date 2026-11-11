import numpy as np, pickle, sim, json, itertools, time, sys
from surrogate import Surrogate
keys,r,se,cx=pickle.load(open("stage_meta.pkl","rb"))
P,Q=sim.f(sim.B3_P),sim.f(sim.B3_Q)
S=Surrogate(cx,r,se,P,Q)
C=np.concatenate([P,Q]).copy()
if len(sys.argv)>1: C=np.array([int(h,16) for h in json.load(open(sys.argv[1]))],dtype=np.uint64).view(np.float64).copy()
def step(c,m): return (np.array([c]).view(np.int64)+int(m)).view(np.float64)[0]
U=np.array([np.spacing(abs(c)) for c in C])
G=S.gain*U[None,:]/se[:,None]   # chi-units per ulp
best=S.chi2(C); print("start chi2",best); t=time.time()
steps=[1,2,3,4,6,8,12,16,24,32,48,64,96,128,192,256,384,512,1024,2048,4096,8192,16384,32768,65536]
for sweep in range(100):
    imp=0
    for i in range(18):
        for m in steps+[-s for s in steps]:
            Cn=C.copy(); Cn[i]=step(C[i],m); v=S.chi2(Cn)
            if v<best-1e-6: best=v; C=Cn; imp+=1
    for i,j in itertools.permutations(range(18),2):
        gi,gj=G[:,i],G[:,j]
        if gj@gj<1e-12: continue
        ratio=-(gi@gj)/(gj@gj)
        for m in (1,2,4,8,16,64,256,-1,-2,-4,-8,-16,-64,-256):
            mj=int(round(ratio*m))
            for e in (-1,0,1):
                Cn=C.copy(); Cn[i]=step(C[i],m); Cn[j]=step(C[j],mj+e); v=S.chi2(Cn)
                if v<best-1e-6: best=v; C=Cn; imp+=1
    print(sweep,"chi2 %.1f moves %d %.0fs"%(best,imp,time.time()-t),flush=True)
    json.dump([sim.hx(v) for v in C],open("surr_fit2.json","w"))
    if imp==0: break
