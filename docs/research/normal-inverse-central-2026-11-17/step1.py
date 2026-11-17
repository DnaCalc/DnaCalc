from prog import *
d=load(1e-6,1e-5)
print(len(d))
q1=np.arange(-4.0719252,-4.0719244,2e-10)
score=np.zeros(len(q1),dtype=np.int32)
for t,ex in d:
    s=t*t
    p1=float(Rt[1])+K*q1
    P=K+p1*s; Q=1.0+q1*s
    score+=((t*(P/Q))*SQ2==ex)
o=np.argsort(-score)[:5]
for i in o: print(score[i],repr(q1[i]))
good=q1[score==score.max()]; print(good.min(),good.max())
