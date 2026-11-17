import sys,json,math
sys.path.insert(0,'/home/ubuntu/codex-runs/fn6')
from fx87 import mul,add,div
from collections import Counter
K=float.fromhex('0x1.c5bf891b4ef6bp-1'); SQ2=math.sqrt(2)
c1=0.886226925452758*math.pi/12
lim=float(sys.argv[1])
d=[(2*(p-0.5),ex) for p,ex in json.load(open('probe2.json')) if 2*(p-0.5)<lim]
C=Counter(); off=Counter()
for t,ex in d:
    s=mul(t,t)
    g={}
    g['a']=mul(mul(t,add(K,mul(c1,s))),SQ2)
    g['b']=mul(add(mul(t,K),mul(mul(t,s),c1)),SQ2)
    g['c']=mul(add(mul(t,K),mul(t,mul(s,c1))),SQ2)
    for k,v in g.items(): C[k]+=(v==ex)
    R=add(K,mul(c1,s)); u=math.ulp(R)
    off[str([k for k in range(-3,4) if mul(mul(t,R+k*u),SQ2)==ex])]+=1
print(len(d),C); print(off.most_common(10))
