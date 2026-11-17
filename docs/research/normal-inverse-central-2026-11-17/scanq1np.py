import sys,json,math
import numpy as np
K=float.fromhex('0x1.c5bf891b4ef6bp-1'); SQ2=math.sqrt(2)
c1=0.886226925452758*math.pi/12
d=[(2*(p-0.5),ex) for p,ex in json.load(open('probe2.json')) if 2*(p-0.5)<1e-6]
d.sort(key=lambda r:-r[0])
pts=d[:80]
q1=np.arange(-100,100,2e-5)
score=np.zeros(len(q1),dtype=np.int32)
for t,ex in pts:
    s=t*t
    P=K+(c1+K*q1)*s
    Q=1.0+q1*s
    x=(t*(P/Q))*SQ2
    score+= (x==ex)
o=np.argsort(-score)[:30]
for i in o: print(score[i], q1[i])
print('q1=0:',score[np.argmin(abs(q1))])
