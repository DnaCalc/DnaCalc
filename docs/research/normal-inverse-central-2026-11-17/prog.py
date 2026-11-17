import sys,json,math
import numpy as np
sys.path.insert(0,'/home/ubuntu/claude-work/erf/pylib')
import mpmath as mp; mp.mp.dps=40
K=float.fromhex('0x1.c5bf891b4ef6bp-1'); SQ2=math.sqrt(2)
N=12
c=[mp.mpf(1)]
for k in range(1,N): c.append(sum(c[m]*c[k-1-m]/((m+1)*(2*m+1)) for m in range(k)))
a=mp.sqrt(mp.pi)/2
Rt=[c[k]/(2*k+1)*a**(2*k+1) for k in range(N)]
def pcoef(qs):
    # given q (q0=1,q1..), p_k = sum_j R_{k-j} q_j  (Taylor matching)
    qs=[mp.mpf(1)]+[mp.mpf(x) for x in qs]
    return [sum(Rt[k-j]*qs[j] for j in range(min(k,len(qs)-1)+1)) for k in range(len(qs))]
def load(lo,hi):
    out=[]
    for f in ['probe2.json','probe3.json']:
        for p,ex in json.load(open(f)):
            t=2*(p-0.5)
            if lo<=t<hi: out.append((t,ex))
    return out
