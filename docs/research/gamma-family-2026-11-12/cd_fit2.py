import load, sim, numpy as np, sys, time, json, itertools
xs,E=load.load(); mode=0
sub=slice(0,None,3)
xs_s,E_s=np.ascontiguousarray(xs[sub]),np.ascontiguousarray(E[sub])
grid=np.array([1,1,8,128,4,1,1,1,1, 2,128,16,4,2,2,1,1,1])
start=sys.argv[1] if len(sys.argv)>1 else None
if start: C=np.array([int(h,16) for h in json.load(open(start))],dtype=np.uint64).view(np.int64).copy()
else: C=np.concatenate([sim.f(sim.B3_P),sim.f(sim.B3_Q)]).view(np.int64).copy()
J=np.load("J.npy")  # per-ulp window-mean sensitivity
def score(Cb,xx=xs_s,EE=E_s):
    Cf=Cb.view(np.float64); d=sim.ulps(sim.b3(Cf[:9],Cf[9:],xx,mode),EE)
    return -int(np.abs(d).sum()), int((d==0).sum())
best=score(C); print("start",best,len(xs_s)); t=time.time()
R=12
for sweep in range(40):
    improved=False
    # single moves in grid units
    for k in range(18):
        cand=[]
        for o in range(-R,R+1):
            if o==0: continue
            Cb=C.copy(); Cb[k]+=o*grid[k]; cand.append((score(Cb),o*grid[k]))
        s,o=max(cand)
        if s>best: best=s; C[k]+=o; improved=True; print(sweep,"single",k,o,best,"%.0fs"%(time.time()-t),flush=True)
    # compensated pair moves: move k by a grid steps, k2 by -J_k/J_k2 * move (least squares over windows)
    for k,k2 in itertools.permutations(range(18),2):
        jk,jk2=J[:,k],J[:,k2]
        if (jk2@jk2)==0: continue
        ratio=-(jk@jk2)/(jk2@jk2)
        cand=[]
        for a in (-4,-2,-1,1,2,4):
            m=a*grid[k]; m2=int(round(ratio*m/grid[k2]))*grid[k2]
            Cb=C.copy(); Cb[k]+=m; Cb[k2]+=m2
            for e in (-grid[k2],0,grid[k2]):
                Cc=Cb.copy(); Cc[k2]+=e; cand.append((score(Cc),(m,m2+e)))
        s,(m,m2)=max(cand)
        if s>best: best=s; C[k]+=m; C[k2]+=m2; improved=True; print(sweep,"pair",k,k2,m,m2,best,"%.0fs"%(time.time()-t),flush=True)
    json.dump([hex(int(v)&0xffffffffffffffff) for v in C],open("cd_fit2.json","w"))
    if not improved: break
print("full",score(C,xs,E),len(xs))
