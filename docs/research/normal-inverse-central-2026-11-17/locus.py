import remez as R
from remez import mp
import sys
Q1=mp.mpf('-4.0719248245644'); Q2=mp.mpf('6.725955')
fix = sys.argv[1]=="fix"
import remezw_weights
R.W=remezw_weights.W[sys.argv[2]]
sys.argv=sys.argv[:1]+sys.argv[2:]
for mn in sys.argv[2:]:
    m,n=map(int,mn.split(','))
    def q1at(b):
        P,Q,E,mx=R.remez(m,n,mp.mpf(0),mp.mpf(b),p0=(mp.sqrt(mp.pi)/2 if fix else None),iters=15,grid=1500)
        return Q,mx
    lo,hi=mp.mpf('0.2'),mp.mpf('0.95')
    Qlo,_=q1at(lo); Qhi,_=q1at(hi)
    if not ((Qlo[1]-Q1)*(Qhi[1]-Q1)<0): print(mn,'no bracket',mp.nstr(Qlo[1],6),mp.nstr(Qhi[1],6),flush=True); continue
    for it in range(22):
        mid=(lo+hi)/2; Qm,mx=q1at(mid)
        if (Qm[1]-Q1)*(Qlo[1]-Q1)>0: lo,Qlo=mid,Qm
        else: hi=mid
    print(mn,'b',mp.nstr(mid,8),'q2',mp.nstr(Qm[2],9),'dq2',mp.nstr(Qm[2]-Q2,4),'E',mp.nstr(mx,3),flush=True)
