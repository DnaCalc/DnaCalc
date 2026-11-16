import sys,json,math
import numpy as np
K=float.fromhex('0x1.c5bf891b4ef6bp-1'); SQ2=math.sqrt(2)
c1=0.886226925452758*math.pi/12
d=[(2*(p-0.5),ex) for p,ex in json.load(open('probe2.json')) if 2*(p-0.5)<float(sys.argv[1])]
lo,hi,st=float(sys.argv[2]),float(sys.argv[3]),float(sys.argv[4])
q1=np.arange(lo,hi,st)
score=np.zeros(len(q1),dtype=np.int32)
for t,ex in d:
    s=t*t
    P=K+(c1+K*q1)*s
    Q=1.0+q1*s
    score+= ((t*(P/Q))*SQ2==ex)
o=np.argsort(-score)[:10]
print(len(d))
for i in o: print(score[i], repr(q1[i]))
