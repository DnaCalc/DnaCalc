import sys; sys.path.insert(0,'/home/ubuntu/claude-work/erf/pylib')
import mpmath as mp
mp.mp.dps=50
K=mp.sqrt(mp.pi)/2
def f(s): return K if s==0 else mp.erfinv(mp.sqrt(s))/mp.sqrt(s)
def solve_rows(rows,rhs):
    n=len(rows[0]); A=mp.matrix(rows); B=mp.matrix(rhs)
    if len(rows)==n: x=mp.lu_solve(A,B)
    else: x=mp.lu_solve(A.T*A,A.T*B)
    return list(x)
def unknown_row(s,fs,m,n,pin):
    r=([] if pin else [mp.mpf(1)])+[s**a for a in range(1,m+1)]+[-fs*s**c for c in range(1,n+1)]
    rhs=fs-(K if pin else 0)
    return r,rhs
def split(x,m,pin):
    if pin: return [K]+x[:m],[mp.mpf(1)]+x[m:]
    return x[:m+1],[mp.mpf(1)]+x[m+1:]
def interp(m,n,b,pin,nodes='cheb'):
    N=m+n+(0 if pin else 1)
    if nodes=='cheb': ss=[(1-mp.cos(mp.pi*(k+mp.mpf(1)/2)/N))*b/2 for k in range(N)]
    elif nodes=='chebx': ss=[(1-mp.cos(mp.pi*k/(N-1)))*b/2 for k in range(N)] if not pin else [(1-mp.cos(mp.pi*(k+1)/N))*b/2 for k in range(N)]
    elif nodes=='uni': ss=[b*(k+1)/N for k in range(N)]
    rows=[];rhs=[]
    for s in ss:
        r,h=unknown_row(s,f(s),m,n,pin); rows.append(r); rhs.append(h)
    return split(solve_rows(rows,rhs),m,pin)
_g={}
def galerkin(m,n,b,pin,test='leg',wt='abs',NQ=80):
    # sum_i wq_i phi_k(s_i) w(s_i) (P - fQ) = 0, Gauss-Legendre quadrature on [0,b]
    key=(str(b),NQ)
    if key not in _g:
        xs,ws=zip(*mp.gauss_legendre(NQ)) if hasattr(mp,'gauss_legendre') else (None,None)
    return None
def dls(m,n,b,pin,N=200,wt='abs',grid='uni'):
    rows=[];rhs=[]
    for k in range(N):
        s = b*(k+1)/N if grid=='uni' else (1-mp.cos(mp.pi*(k+mp.mpf(1)/2)/N))*b/2
        fs=f(s); r,h=unknown_row(s,fs,m,n,pin); w=1 if wt=='abs' else 1/fs
        rows.append([w*v for v in r]); rhs.append(w*h)
    return split(solve_rows(rows,rhs),m,pin)
Q1=mp.mpf('-4.0719248245643'); Q2=mp.mpf('6.72595946')
def report(name,fn):
    P,Q=fn(mp.mpf('0.7225'))
    print(name,'@0.7225 q1',mp.nstr(Q[1],14),'q2',mp.nstr(Q[2],10),flush=True)
    lo,hi=mp.mpf('0.6'),mp.mpf('0.85'); glo=fn(lo)[1][1]-Q1; ghi=fn(hi)[1][1]-Q1
    if (glo>0)==(ghi>0): print('   no bracket',mp.nstr(glo,4),mp.nstr(ghi,4)); return
    for _ in range(40):
        mid=(lo+hi)/2; gm=fn(mid)[1][1]-Q1
        if (gm>0)==(glo>0): lo,glo=mid,gm
        else: hi=mid
    P,Q=fn(mid); print('   q1-match b',mp.nstr(mid,12),'dq2',mp.nstr(Q[2]-Q2,5),'q3',mp.nstr(Q[3],8),flush=True)
if __name__=='__main__':
    for pin in [True,False]:
        for nodes in ['cheb','chebx','uni']:
            report('interp %s %s'%(nodes,pin), lambda b: interp(7,7,b,pin,nodes))
