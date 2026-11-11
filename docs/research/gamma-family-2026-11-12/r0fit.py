import numpy as np, sim, load, math, itertools, json, sys, time
xs,E=load.load(); W=len(xs)//1024
X=xs.reshape(W,1024); EE=E.reshape(W,1024); cx=X[:,512]
lo,hi=float(sys.argv[1]),float(sys.argv[2]); mode=int(sys.argv[3]); out=sys.argv[4]
R0=[w for w in range(W) if lo<cx[w]<hi]
xx=np.ascontiguousarray(X[R0].ravel()); ee=EE[R0].ravel()
P,Q=sim.f(sim.B3_P).copy(),sim.f(sim.B3_Q).copy()
if len(sys.argv)>5:
    C=np.array([int(h,16) for h in json.load(open(sys.argv[5]))],dtype=np.uint64).view(np.float64).copy()
else:
    P[0]=round(P[0]/2.0**-40)*2.0**-40-4*2.0**-40; Q[0]=1.0+5*2.0**-39
    C=np.concatenate([P,Q])
def score(C):
    d=sim.ulps(sim.b3(C[:9],C[9:],xx,mode),ee); return int((d==0).sum()), -int(np.abs(d).sum())
# effective step per coefficient in R0: the stage grid (ulp of the stage value) over ulp(c)
def stage_grid(C):
    x=0.5*(lo+hi); g=[]
    for poly in (C[:9],C[9:]):
        v=poly[8]; S={8:v}
        for k in range(7,-1,-1): v=v*x+poly[k]; S[k]=v
        for k in range(9):
            if k==8: g.append(1); continue
            prec=53 if (k>0 or (mode & 13)) else 64
            gs=2.0**(math.floor(math.log2(abs(S[k])))-prec+1)
            g.append(max(1,int(round(gs/np.spacing(abs(poly[k]))))))
    return np.array(g)
G=stage_grid(C); print("grid",G.tolist())
def step(C,i,m):
    Cn=C.copy(); Cn[i]=(np.array([C[i]]).view(np.int64)+int(m)).view(np.float64)[0]; return Cn
best=score(C); print("start",best,len(xx)); t=time.time()
for sweep in range(30):
    imp=0
    for i in range(18):
        for m in (1,2,3,4,6,8,-1,-2,-3,-4,-6,-8):
          for sub in (0,1,-1,2,-2,4,-4,8,-8):
            if G[i]==1 and sub: continue
            Cn=step(C,i,m*G[i]+sub if sub==0 else sub); s=score(Cn)
            if s>best: best=s; C=Cn; imp+=1
    for i,j in itertools.combinations(range(18),2):
        for mi in (1,-1):
            for mj in (1,-1,2,-2):
                Cn=step(step(C,i,mi*G[i]),j,mj*G[j]); s=score(Cn)
                if s>best: best=s; C=Cn; imp+=1
    print(sweep,best,"%.3f"%(best[0]/len(xx)),imp,"%.0fs"%(time.time()-t),flush=True)
    json.dump([sim.hx(v) for v in C],open(out,"w"))
    if not imp: break
