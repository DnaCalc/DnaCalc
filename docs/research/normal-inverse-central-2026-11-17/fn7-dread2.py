# phase_i = Q*E_i + D*sig_i (mod 1), E_i = (r_i - Rtrue_i)*2^53 (absolute units), sig_i=(s_i-s_c)*2^53
# Q = stored denominator at s_c (grid 2^-53), D = (P-Q)'(s_c)
import numpy as np, sys, json
from multiprocessing import Pool
c=np.load('central.npz'); T=c['t'];SRC=c['src'];WIN=c['win']
import os
o=np.fromfile(os.environ.get('PREP','prep.bin')).reshape(-1,7)
def window(S,w):
    m=(SRC==S)&(WIN==w)&(o[:,2]==1)
    return T[m],o[m,3],o[m,0],o[m,1]
def read(args):
    S,w,qlo,qhi=args
    t,r,hi,lo=window(S,w)
    if len(t)<200: return None
    s=t*t; ic=len(s)//2; sc=s[ic]
    E=((r-hi)-lo)*2.0**53
    sig=(s-sc)*2.0**53
    j=np.round(sig*4).astype(np.int64); j-=j.min(); L=j.max()+1
    nfft=1<<int(np.ceil(np.log2(L*4)))
    best=(0,0,0)
    for Q in np.arange(qlo,qhi,0.0005):
        z=np.exp(2j*np.pi*Q*E)
        A=np.bincount(j,weights=z.real,minlength=L)+1j*np.bincount(j,weights=z.imag,minlength=L)
        F=np.abs(np.fft.fft(A,nfft)); b=F.argmax()
        if F[b]>best[0]: best=(F[b],Q,b)
    D=(-4*best[2]/nfft)%4; Q=best[1]
    def coh(Q,D): return np.abs(np.exp(2j*np.pi*(Q*E+D*sig)).sum())/len(s)
    step=4/nfft; dq=0.0005
    for it in range(4):
        Ds=D+np.linspace(-step,step,41); D=Ds[int(np.argmax([coh(Q,x) for x in Ds]))]
        Qs=Q+np.linspace(-dq,dq,41); Q=Qs[int(np.argmax([coh(x,D) for x in Qs]))]
        step/=10; dq/=10
    if D>2: D-=4
    return dict(S=S,w=w,n=len(s),s=float(sc),R=float(r[ic]),coh=float(coh(Q,D)),Q=float(Q),D=float(D),sspan=float(sig.max()-sig.min()))
if __name__=='__main__':
    smin=float(sys.argv[1]); qlo,qhi=float(sys.argv[2]),float(sys.argv[3])
    wins=[]
    for S in [41,43,49]:
        m=SRC==S
        for w in np.unique(WIN[m]):
            mm=m&(WIN==w); s=T[mm]**2
            if s.max()-s.min()<1e-9 and smin<s.min()<0.5 and (s.min()>0.25 or s.max()<0.25): wins.append((S,int(w),qlo,qhi))
    with Pool(10) as p:
        out=[d for d in p.map(read,wins) if d]
    out.sort(key=lambda d:d['s'])
    for d in out: print(d['S'],d['w'],d['n'],round(d['s'],5),round(d['R'],5),round(d['coh'],3),round(d['Q'],5),round(d['D'],6))
    json.dump(out,open(sys.argv[4],'w'))
