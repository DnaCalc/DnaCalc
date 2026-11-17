import sys,json,math
sys.path.insert(0,'/home/ubuntu/codex-runs/fn6')
from fx87 import mul,add,div
from collections import Counter
K=float.fromhex('0x1.c5bf891b4ef6bp-1'); SQ2=math.sqrt(2)
c1=0.886226925452758*math.pi/12
lim=float(sys.argv[1])
d=[(2*(p-0.5),ex) for p,ex in json.load(open('probe2.json')) if 2*(p-0.5)<lim]
C=Counter()
for t,ex in d:
    s=mul(t,t)
    ws={}
    ws['s']=s
    ws['-log((1-t)(1+t)) f64']=-math.log((1-t)*(1+t))
    ws['-log(st((1-t)*(1+t)))']=-math.log(mul(add(1,-t),add(1,t)))
    ws['-log1p(-s)']=-math.log1p(-s)
    ws['-log(1-s)']=-math.log(1-s)
    ws['-log(st(1-s))']=-math.log(add(1,-s))
    for k,w in ws.items():
        C[k]+= (mul(mul(t,add(K,mul(c1,w))),SQ2)==ex)
print(len(d)); [print(v,k) for k,v in C.most_common()]
