# Fast surrogate of the window-mean residual (Excel - model) as a function of the coefficient table.
import numpy as np, math, sim
from fractions import Fraction as Fr
def rn_grid(c, g):
    """round c (float) to a multiple of g (power of two), ties to even multiple"""
    q=Fr(c)/Fr(g); n=math.floor(q); f=q-n
    if f>Fr(1,2) or (f==Fr(1,2) and n%2==1): n+=1
    return n*Fr(g)
def rn_tie_avg(q,g):
    q=q/Fr(g); n=math.floor(q); f=q-n
    if f==Fr(1,2): return (n+Fr(1,2))*Fr(g)
    if f>Fr(1,2): n+=1
    return n*Fr(g)
def eff(c,gs,gp):
    if gp>gs: return rn_grid(c,gs)
    if gp==gs: return rn_tie_avg(Fr(c),gs)
    n=int(round(gs/gp)); tot=Fr(0)
    for j in range(n): tot+=rn_tie_avg(Fr(c)+j*Fr(gp),gs)-j*Fr(gp)
    return tot/n
def rn_grid_fr(q,g):
    q=q/Fr(g); n=math.floor(q); f=q-n
    if f>Fr(1,2) or (f==Fr(1,2) and n%2==1): n+=1
    return n*Fr(g)
class Surrogate:
    def __init__(self, cx, r, se, P, Q, last_stored=(False,False)):
        self.cx, self.r, self.se = cx, r, se
        self.base = np.concatenate([P,Q]).copy()
        W=len(cx); self.W=W
        # per window, per coefficient index i (0..17): gain (output ulps per unit), grid (None if random)
        self.gain=np.zeros((W,18)); self.grid=[[None]*18 for _ in range(W)]
        for w,x in enumerate(cx):
            x=float(x)
            def stages(c):
                v=c[8]; S={8:v}; Pp={}
                for k in range(7,-1,-1): Pp[k]=v*x; v=v*x+c[k]; S[k]=v
                return S,Pp
            SP,PP=stages(P); SQ,PQ=stages(Q)
            N,D=SP[0],SQ[0]; y=(x-2)*N/D; uo=np.spacing(y)
            for i in range(18):
                poly,k=(0,i) if i<9 else (1,i-9)
                S,Pr=(SP,PP) if poly==0 else (SQ,PQ)
                g=(x-2)*x**k/D if poly==0 else -(x-2)*N*x**k/D**2
                self.gain[w,i]=g/uo
                if k==8: continue
                bs=math.floor(math.log2(abs(S[k]))); bp=math.floor(math.log2(abs(Pr[k])))
                prec=53 if (k>0 or last_stored[poly]) else 64
                gs=2.0**(bs-prec+1); gp=2.0**(bp-52)
                self.grid[w][i]=(gs,gp)
        self.cache={}
    def dC(self, i, c):
        """per-window change of the effective constant of coefficient i when it takes value c"""
        key=(i,c)
        if key in self.cache: return self.cache[key]
        c0=self.base[i]; out=np.empty(self.W)
        memo={}
        for w in range(self.W):
            g=self.grid[w][i]
            if g is None: out[w]=c-c0
            else:
                if g not in memo: memo[g]=float(eff(c,*g)-eff(c0,*g))
                out[w]=memo[g]
        self.cache[key]=out; return out
    def predict(self, C):
        tot=np.zeros(self.W)
        for i in range(18):
            if C[i]!=self.base[i]: tot+=self.gain[:,i]*self.dC(i,C[i])
        return tot
    def chi2(self, C):
        return float((((self.r-self.predict(C))/self.se)**2).sum())
