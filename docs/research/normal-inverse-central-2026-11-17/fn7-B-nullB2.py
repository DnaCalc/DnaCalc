import sys; sys.path.insert(0,'/home/ubuntu/claude-work/erf/pylib')
import mpmath as mp, numpy as np, subprocess
mp.mp.dps=80
a,b=mp.mpf('0.5'),mp.mpf('0.73')
N=13; vs=[(a+b)/2-(b-a)/2*mp.cos(mp.pi*(k+mp.mpf(1)/2)/N) for k in range(N)]
G={'R':lambda v: mp.erfinv(mp.sqrt(v))/mp.sqrt(v),'y':lambda v: mp.erfinv(mp.sqrt(v)),'Rx':lambda v: mp.sqrt(2)*mp.erfinv(mp.sqrt(v))/mp.sqrt(v),'x':lambda v: mp.sqrt(2)*mp.erfinv(mp.sqrt(v))}
FORMS={'R':[3,1],'y':[0],'Rx':[4,6],'x':[5]}
alphas=np.concatenate([-np.logspace(-20,-9,111),[0],np.logspace(-20,-9,111)])
c=mp.mpf(sys.argv[1]) if len(sys.argv)>1 else mp.mpf(0)
for kind,g in G.items():
    fv=[g(v) for v in vs]; us=[v-c for v in vs]
    for m in range(2,11):
        n=12-m
        A=mp.matrix([[u**i for i in range(m+1)]+[-f*u**k for k in range(1,n+1)] for u,f in zip(us,fv)])
        x0=mp.lu_solve(A,mp.matrix(fv)); U,S,V=mp.svd_r(A); vmin=V[V.rows-1,:]
        best={}
        for al in alphas:
            x=[x0[i]+mp.mpf(al)*vmin[i] for i in range(13)]
            P=x[:m+1]; Q=[mp.mpf(1)]+x[m+1:]
            open('cB5.txt','w').write('%d %d\n'%(m,n)+' '.join(float(v).hex() for v in P)+'\n'+' '.join(float(v).hex() for v in Q)+'\n')
            for f in FORMS[kind]:
                out=subprocess.run(['./emuB4','cB5.txt',str(f),repr(float(c))],capture_output=True,text=True).stdout.strip()
                h=int(out.split()[0])
                if f not in best or h>best[f][0]: best[f]=(h,al)
        print(kind,(m,n),'smin %.1e'%float(S[len(S)-1]),{f:(v[0],'%.2e'%v[1]) for f,v in best.items()},flush=True)
