import load, sim, numpy as np, sys, time, json
xs,E=load.load(); mode=int(sys.argv[1]) if len(sys.argv)>1 else 0
R=int(sys.argv[2]) if len(sys.argv)>2 else 32
sub=slice(0,None,3)
xs_s,E_s=np.ascontiguousarray(xs[sub]),np.ascontiguousarray(E[sub])
C=np.concatenate([sim.f(sim.B3_P),sim.f(sim.B3_Q)]).view(np.int64).copy()
def score(Cb,xx=xs_s,EE=E_s):
    Cf=Cb.view(np.float64); d=sim.ulps(sim.b3(Cf[:9],Cf[9:],xx,mode),EE)
    return int((d==0).sum()), -int(np.abs(d).sum())
best=score(C); print("start",best,len(xs_s)); t=time.time()
for sweep in range(int(sys.argv[3]) if len(sys.argv)>3 else 6):
    improved=False
    for k in list(range(18)):
        cand=[]
        for o in range(-R,R+1):
            if o==0: continue
            Cb=C.copy(); Cb[k]+=o; cand.append((score(Cb),o))
        s,o=max(cand)
        if s>best: best=s; C[k]+=o; improved=True; print(sweep,k,o,best,"%.0fs"%(time.time()-t),flush=True)
    if not improved: break
json.dump([hex(int(v)&0xffffffffffffffff) for v in C],open(f"cd_fit_m{mode}.json","w"))
print("full",score(C,xs,E),len(xs))
