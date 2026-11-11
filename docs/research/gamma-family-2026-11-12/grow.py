# Region growing: start from the windows near 2.5, add windows outward; when a window falls below the bar,
# search the coefficients whose stage binade signature changed against the already solved windows.
import numpy as np, sim, load, math, json, sys, time
xs,E=load.load(); W=len(xs)//1024
X=xs.reshape(W,1024); EE=E.reshape(W,1024)
cx=X[:,512]
C=np.concatenate([sim.f(sim.B3_P),sim.f(sim.B3_Q)]).copy()
def sig(C,x):
    s=[]
    for poly in (C[:9],C[9:]):
        v=poly[8]
        for k in range(7,-1,-1):
            p=v*x; v=p+poly[k]
            s.append((k,"p",math.floor(math.log2(abs(p))))); s.append((k,"s",math.floor(math.log2(abs(v)))))
    return s
def match(C,ws):
    xx=np.ascontiguousarray(X[ws].ravel()); ee=EE[ws].ravel()
    d=sim.ulps(sim.b3(C[:9],C[9:],xx,0),ee); return (d==0).mean()
def per_window(C,ws):
    xx=np.ascontiguousarray(X[ws].ravel()); ee=EE[ws].ravel()
    d=sim.ulps(sim.b3(C[:9],C[9:],xx,0),ee).reshape(len(ws),1024); return (d==0).mean(1)
order=sorted(range(256), key=lambda w: abs(cx[w]-2.5))
bar=float(sys.argv[1]) if len(sys.argv)>1 else 0.9
R=int(sys.argv[2]) if len(sys.argv)>2 else 300
solved=[]
t=time.time()
for w in order:
    m=per_window(C,[w])[0]
    if m>=bar or not solved:
        solved.append(w); continue
    # which coefficient indices have a changed signature vs any solved window?
    s_new=sig(C,cx[w]); changed=set()
    for v in solved:
        s_old=sig(C,cx[v])
        for i,(a,b) in enumerate(zip(s_new,s_old)):
            if a!=b:
                poly=i//16; k=(i%16)//2; changed.add(poly*9+k)
                if a[1]=="p" and k<8: changed.add(poly*9+k+1)  # product of stage k+1 times x
    best=(match(C,solved+[w]),None,None); base=best[0]
    for i in sorted(changed):
        for j in range(-R,R+1):
            if j==0: continue
            Cn=C.copy(); Cn[i]=(np.array([C[i]]).view(np.int64)+j).view(np.float64)[0]
            v=match(Cn,solved+[w])
            if v>best[0]: best=(v,i,j)
    if best[1] is not None:
        C[best[1]]=(np.array([C[best[1]]]).view(np.int64)+best[2]).view(np.float64)[0]
    mw=per_window(C,[w])[0]; ms=match(C,solved)
    print("w %.4f before %.3f after %.3f solved-set %.4f changed %s move %s %s %.0fs"%(cx[w],m,mw,ms,sorted(changed),best[1],best[2],time.time()-t),flush=True)
    solved.append(w)
    json.dump([sim.hx(v) for v in C],open("grow.json","w"))
print("final all-window match", match(C,list(range(W))))
