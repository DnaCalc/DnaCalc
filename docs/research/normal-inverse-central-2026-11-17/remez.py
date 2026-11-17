import sys; sys.path.insert(0,'/home/ubuntu/claude-work/erf/pylib')
import mpmath as mp
mp.mp.dps=45
_cache={}
def Rf(s):
    if s in _cache: return _cache[s]
    if s==0: v=mp.sqrt(mp.pi)/2
    else:
        t=mp.sqrt(s); v=mp.erfinv(t)/t
    _cache[s]=v; return v
W=lambda x: Rf(x)
def solve(xs,fx,m,n,E,p0):
    A=[];bb=[]
    for i,x in enumerate(xs):
        w=fx[i]+(-1)**i*E*W(x)
        row=([] if p0 is not None else [mp.mpf(1)])+[x**k for k in range(1,m+1)]+[-w*x**k for k in range(1,n+1)]
        A.append(row); bb.append(w-(p0 if p0 is not None else 0))
    k=len(A[0])
    sol=mp.lu_solve(mp.matrix(A[:k]),mp.matrix(bb[:k]))
    r=sum(A[k][j]*sol[j] for j in range(k))-bb[k]
    return sol,r
def coeffs(sol,m,n,p0):
    sol=list(sol)
    if p0 is not None: P=[p0]+sol[:m]; Q=[mp.mpf(1)]+sol[m:]
    else: P=sol[:m+1]; Q=[mp.mpf(1)]+sol[m+1:]
    return P,Q
def ev(C,x):
    v=mp.mpf(0)
    for c in reversed(C): v=v*x+c
    return v
def err(P,Q,x): return (ev(P,x)/ev(Q,x)-Rf(x))/W(x)
def remez(m,n,a,b,p0=None,iters=25,grid=3000,verbose=False):
    N=m+n+2-(1 if p0 is not None else 0)
    xs=[(a+b)/2-(b-a)/2*mp.cos(mp.pi*i/(N-1)) for i in range(N)]
    if p0 is not None: xs=[(a+b)/2-(b-a)/2*mp.cos(mp.pi*(i+1)/(N)) for i in range(N)]  # x=a is exact, skip
    G=[a+(b-a)*(1-mp.cos(mp.pi*i/(grid-1)))/2 for i in range(grid)]
    for it in range(iters):
        fx=[Rf(x) for x in xs]
        E0=mp.mpf(0); E1=mp.mpf('1e-18')
        s0,r0=solve(xs,fx,m,n,E0,p0); s1,r1=solve(xs,fx,m,n,E1,p0)
        for k in range(60):
            E2=E1-r1*(E1-E0)/(r1-r0); E0,r0=E1,r1; E1=E2; s1,r1=solve(xs,fx,m,n,E1,p0)
            if abs(r1)<mp.mpf(10)**-40: break
        P,Q=coeffs(s1,m,n,p0)
        e=[err(P,Q,x) for x in G]
        # find alternating extrema
        ext=[]
        i=0
        # segments by sign
        segs=[]; cur=[0]
        for j in range(1,grid):
            if (e[j]>=0)==(e[cur[0]]>=0): cur.append(j)
            else: segs.append(cur); cur=[j]
        segs.append(cur)
        if p0 is not None: segs=[sg for sg in segs if not (len(sg)==1 and sg[0]==0)]
        pts=[max(sg,key=lambda j:abs(e[j])) for sg in segs]
        maxe=max(abs(e[j]) for j in pts)
        if verbose: print(it,'E',mp.nstr(E1,6),'max',mp.nstr(maxe,6),'nseg',len(pts))
        if len(pts)<N:
            break
        # choose N consecutive with largest min
        best=None
        for st in range(len(pts)-N+1):
            sel=pts[st:st+N]; mn=min(abs(e[j]) for j in sel)
            if best is None or mn>best[0]: best=(mn,sel)
        new=[G[j] for j in best[1]]
        if all(abs(new[i]-xs[i])<mp.mpf(10)**-12 for i in range(N)) and it>3: xs=new; break
        xs=new
    return P,Q,E1,maxe
if __name__=='__main__':
    m,n=int(sys.argv[1]),int(sys.argv[2]); b=mp.mpf(sys.argv[3]) if len(sys.argv)>3 else mp.mpf('0.5')
    fix=None if len(sys.argv)<5 else mp.sqrt(mp.pi)/2
    P,Q,E,mx=remez(m,n,mp.mpf(0),b,p0=fix,verbose=True)
    print('E',mp.nstr(E,8),'max',mp.nstr(mx,8))
    print('P',[mp.nstr(c,18) for c in P]); print('Q',[mp.nstr(c,18) for c in Q])
