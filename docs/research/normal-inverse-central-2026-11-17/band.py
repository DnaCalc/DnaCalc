# band scan: ask Excel dense points near t0, scan (dQ, e) offsets of Q(s), P(s)=R_true*Q*(1+e)
import sys,json,math,random,pickle
import numpy as np
sys.path.insert(0,'/home/ubuntu/claude-work/erf/pylib'); sys.path.insert(0,'/home/ubuntu/codex-runs/fn6')
import mpmath as mp; mp.mp.dps=40
from fx87 import mul
SQ2=math.sqrt(2)
def Rtrue(s):
    t=mp.sqrt(mp.mpf(s)); return mp.erfinv(t)/t
def get_band(t0,rel,n,seed):
    from ts import ask
    rnd=random.Random(seed)
    ps=sorted(set(0.5+t0*(1+rnd.uniform(-rel,rel))/2 for _ in range(n)))
    ans=ask('NORM.S.INV',[(p,) for p in ps])
    return [(2*(p-0.5),a) for p,a in zip(ps,ans)]
def scan(pts,Qm,W,sub=32,esteps=64):
    """Qm: callable s->mp value (model Q). returns best (dQ,e) grid scores"""
    S=[mul(t,t) for t,_ in pts]
    s0=S[len(S)//2]
    Q0=Qm(s0); u=mp.mpf(math.ulp(float(Q0)))
    dq=np.arange(-W*sub,W*sub+1)
    ee=np.arange(-esteps,esteps+1)
    score=np.zeros((len(dq),len(ee)),dtype=np.int32)
    K=dq.astype(np.float64); E=ee.astype(np.float64)*2.0**-58
    for (t,ex),s in zip(pts,S):
        Qs=Qm(s); R=Rtrue(s)
        Qh=float(Qs); Ql=float(Qs-mp.mpf(Qh)); uf=float(u)/sub
        Qv=Qh+(Ql+K*uf)
        P0=R*Qs; Ph=float(P0); Pl=float(P0-mp.mpf(Ph)); dPf=float(R*u/sub)
        Pv=Ph+((Pl+K[:,None]*dPf)+Ph*E[None,:])
        x=(t*(Pv/Qv[:,None]))*SQ2
        score+=(x==ex)
    return dq/sub, ee*2.0**-58, score, s0
