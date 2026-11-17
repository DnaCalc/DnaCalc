import sys,json,math,random
sys.path.insert(0,'/home/ubuntu/codex-runs/fn6')
from fx87 import mul,add,div
from collections import Counter,defaultdict
SQ2=math.sqrt(2)
def horner(C,s):
    v=C[-1]
    for c in reversed(C[:-1]): v=add(mul(v,s),c)
    return v
def model(co,t):
    s=mul(t,t)
    return mul(mul(t,div(horner(co['P'],s),horner(co['Q'],s))),SQ2)
if __name__=='__main__':
    co=json.load(open(sys.argv[1]))
    d=json.load(open('probe3.json')); random.seed(99); sub=random.sample(d,int(sys.argv[2]))
    res=defaultdict(Counter)
    for p,ex in sub:
        t=2*(p-0.5); b=int(t*10)/10
        res[b]['n']+=1; res[b]['hit']+=(model(co,t)==ex)
    tot=sum(r['hit'] for r in res.values()); n=sum(r['n'] for r in res.values())
    print(tot,n,{b:round(r['hit']/r['n'],3) for b,r in sorted(res.items())})
