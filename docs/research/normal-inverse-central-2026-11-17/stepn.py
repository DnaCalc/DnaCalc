from prog import *
# usage: stepn.py lo hi  'q1lo,q1hi,step' 'q2lo,q2hi,step' ...
lo,hi=float(sys.argv[1]),float(sys.argv[2])
d=load(lo,hi); print(len(d))
axes=[np.arange(*map(float,a.split(','))) for a in sys.argv[3:]]
G=np.meshgrid(*axes,indexing='ij')
n=len(G)
Rf=[float(x) for x in Rt]
# p_k = sum_{j=0..k} R_{k-j} q_j, q_0=1
qs=[1.0]+list(G)
ps=[sum(Rf[k-j]*qs[j] for j in range(k+1)) for k in range(n+1)]
ps[0]=K
score=np.zeros(G[0].shape,dtype=np.int32)
for t,ex in d:
    s=t*t
    P=ps[n]
    for k in range(n-1,-1,-1): P=P*s+ps[k]
    Q=qs[n]
    for k in range(n-1,-1,-1): Q=Q*s+qs[k]
    score+=((t*(P/Q))*SQ2==ex)
m=score.max(); W=np.where(score==m)
print('max',m)
for i,a in enumerate(axes): print('q%d'%(i+1), repr(a[W[i]].min()), repr(a[W[i]].max()))
idx=np.argsort(-score,axis=None)[:5]
for i in idx:
    ii=np.unravel_index(i,score.shape); print(score[ii],[repr(axes[k][ii[k]]) for k in range(n)])
