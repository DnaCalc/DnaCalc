import sys,json,math,random
sys.path.insert(0,'/home/ubuntu/claude-work/erf/pylib')
import mpmath as mp; mp.mp.dps=40
K=mp.mpf(float.fromhex('0x1.c5bf891b4ef6bp-1')); SQ2=mp.mpf(math.sqrt(2))
q1=mp.mpf('-4.0719248245644'); q2=mp.mpf('6.725955')
m,n=int(sys.argv[1]),int(sys.argv[2])
d=json.load(open('probe3.json'))
random.seed(2); sub=random.sample(d,int(sys.argv[3]))
rows=[];rhs=[]
for p,ex in sub:
    t=mp.mpf(2*(p-0.5)); s=t*t
    R=mp.mpf(ex)/SQ2/t; w=1/R
    # R*(1+q1 s+q2 s^2+sum q_j s^j) - (K+sum p_i s^i)=0
    rows.append([-(s**i)*w for i in range(1,m+1)]+[R*s**j*w for j in range(3,n+1)])
    rhs.append(-(R*(1+q1*s+q2*s*s)-K)*w)
x,res=mp.qr_solve(mp.matrix(rows),mp.matrix(rhs))
P=[K]+[x[i] for i in range(m)]; Q=[mp.mpf(1),q1,q2]+[x[m+j] for j in range(n-2)]
print('P',[float(c) for c in P]); print('Q',[float(c) for c in Q])
json.dump({'P':[float(c) for c in P],'Q':[float(c) for c in Q]},open(f'lsfix{m}{n}.json','w'))
