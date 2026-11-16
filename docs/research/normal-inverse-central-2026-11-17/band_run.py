from band import *
import json,sys
co=json.load(open('coef_cur.json'))
Qc=[mp.mpf(c) for c in co['Q']]
def Qm(s):
    v=mp.mpf(0)
    for c in reversed(Qc): v=v*mp.mpf(s)+c
    return v
t0=float(sys.argv[1]); rel=float(sys.argv[2]); W=int(sys.argv[3]); n=int(sys.argv[4]) if len(sys.argv)>4 else 400
pts=get_band(t0,rel,n,int(t0*1e9)%100000)
dq,ee,score,s0=scan(pts,Qm,W)
m=score.max(); I,J=np.where(score==m)
print('t0',t0,'s0',s0,'n',len(pts),'max',m,'dQ ulp range',dq[I].min(),dq[I].max(),'e range',ee[J].min()/2**-53,ee[J].max()/2**-53)
# second best for separation
flat=np.sort(score.ravel())[::-1]; print('top scores',flat[:5])
res=json.load(open('bands.json')) if __import__('os').path.exists('bands.json') else []
u=math.ulp(float(Qm(s0)))
res.append({'t0':t0,'s0':s0,'n':len(pts),'max':int(m),'Q':str(Qm(s0)+mp.mpf(float((dq[I].min()+dq[I].max())/2))*u),'dQlo':float(dq[I].min()),'dQhi':float(dq[I].max()),'ulp':u,'e':float((ee[J].min()+ee[J].max())/2)})
json.dump(res,open('bands.json','w'),indent=0)
