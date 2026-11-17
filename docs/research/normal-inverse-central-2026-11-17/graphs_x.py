import sys,json,math
sys.path.insert(0,'/home/ubuntu/codex-runs/fn6')
from fx87 import *
from collections import Counter
K=float.fromhex('0x1.c5bf891b4ef6bp-1'); SQ2=math.sqrt(2)
c1=0.886226925452758*math.pi/12
lim=float(sys.argv[1])
d=[(2*(p-0.5),ex) for p,ex in json.load(open('probe2.json')) if 2*(p-0.5)<lim]
C=Counter()
for t,ex in d:
    T=X(t); S=xmul(T,T); s=xst(S)
    g={}
    g['a']=mul(mul(t,add(K,mul(c1,s))),SQ2)
    g['ext R, st y']=mul(xst(xmul(T,xadd(X(K),xmul(X(c1),S)))),SQ2)
    g['ext R, s stored, st y']=mul(xst(xmul(T,xadd(X(K),xmul(X(c1),X(s))))),SQ2)
    g['ext y=tK+t*s*c1']=mul(xst(xadd(xmul(T,X(K)),xmul(xmul(T,S),X(c1)))),SQ2)
    g['R stored from ext, y']=mul(mul(t,xst(xadd(X(K),xmul(X(c1),S)))),SQ2)
    for k,v in g.items(): C[k]+=(v==ex)
print(len(d)); [print(v,k) for k,v in C.most_common()]
