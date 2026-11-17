import sys; sys.path.insert(0,'/home/ubuntu/claude-work/erf/pylib')
import mpmath as mp, subprocess, numpy as np
mp.mp.dps=60
def f(s): return mp.erfinv(mp.sqrt(s))/mp.sqrt(s)
b=mp.mpf('0.73'); N=15
ss=[(1-mp.cos(mp.pi*(k+mp.mpf(1)/2)/N))*b/2 for k in range(N)]
fs=[f(s) for s in ss]
A=mp.matrix([[mp.mpf(1)]+[s**a for a in range(1,8)]+[-fv*s**c for c in range(1,8)] for s,fv in zip(ss,fs)])
x0=mp.lu_solve(A,mp.matrix(fs))
U,S,V=mp.svd_r(A)
print('sing',[mp.nstr(v,3) for v in S])
vs=[V[V.rows-1-j,:] for j in range(3)]  # rows of V^T? mp.svd_r returns V as V^T rows
def coefs(alphas):
    x=[x0[i]+sum(a*vs[j][i] for j,a in enumerate(alphas)) for i in range(15)]
    return x[:8],[mp.mpf(1)]+x[8:]
def score(alphas,tag=''):
    P,Q=coefs(alphas)
    open('c_n.txt','w').write(' '.join(float(c).hex() for c in P)+'\n'+' '.join(float(c).hex() for c in Q)+'\n')
    out=subprocess.run(['./emu','c_n.txt','0'],capture_output=True,text=True).stdout.strip().split('\n')
    return int(out[-1].split()[1].split('/')[0])
# scale: unit vector components; coefficient ulp ~1e-16 -> alpha range
# check null vector direction vs c_p94 - c_73
xp=[float.fromhex(v) for v in open('c_p94.txt').read().split()]
d=[mp.mpf(xp[i if i<8 else i+1]) - x0[i] for i in range(15)]
proj=[sum(d[i]*vs[j][i] for i in range(15)) for j in range(3)]
print('c_p94 - c73 projections on v1..v3', [mp.nstr(p,4) for p in proj], 'norm', mp.nstr(mp.sqrt(sum(v*v for v in d)),4))
best=None
for a in np.linspace(-float(sys.argv[1]),float(sys.argv[1]),int(sys.argv[2])):
    sc=score([mp.mpf(a)]); 
    if best is None or sc>best[0]: best=(sc,a)
    print('%.3e %d'%(a,sc),flush=True)
print('best',best)
