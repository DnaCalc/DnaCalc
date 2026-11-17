import gzip,json,io,struct,numpy as np
E='/home/ubuntu/projects/DnaCalc/DnaCalc-Research/evidence/'
def h2f(h): return struct.unpack('>d',bytes.fromhex(h))[0]
def u2f(u): return np.frombuffer(np.asarray(u,dtype=np.uint64).tobytes(),dtype=np.float64)
P=[];X=[];SRC=[];WIN=[]
def add(ps,xs,src,win):
    P.extend(ps);X.extend(xs);SRC.extend([src]*len(ps));WIN.extend([win]*len(ps))
# R41
z=np.load(io.BytesIO(gzip.open(E+'round41/retained/normal-dense-outputs.npz.gz').read()))['answers']
f=json.load(gzip.open(E+'round41/retained/normal-dense-frozen.json.gz'))
for i,w in enumerate(f['windows']):
    st=int(w['start'],16); ps=u2f(np.arange(st,st+w['count'],dtype=np.uint64)); xs=u2f(z[i])
    add(list(ps),list(xs),41,i)
# R43
d=json.load(gzip.open(E+'round43/retained/fresh/NORM.S.INV.lattice.replies.json.gz'))
for i,w in enumerate(d['windows']):
    st=int(w['start'],16); n=len(w['rows'])
    ps=u2f(np.arange(st,st+n,dtype=np.uint64)); xs=[h2f(r) if isinstance(r,str) and len(r)==16 else float('nan') for r in w['rows']]
    add(list(ps),xs,43,i)
# R49
d=json.load(gzip.open(E+'round49/retained/normal-probe-result.json.gz'))
ins=d['inputs']; ans=d['answers']
print('r49 answers sample',ans[:2], len(ins), len(ans))
for k,(a,b) in enumerate(zip(ins,ans)):
    try: x=h2f(b["bits"])
    except Exception: x=float('nan')
    add([h2f(a)],[x],49,k//2048)
# fn6 probes
for j,fn in enumerate(['probe1.json','probe2.json','probe3.json','probe4.json']):
    for p,x in json.load(open('/home/ubuntu/codex-runs/fn6/'+fn)):
        add([p],[x if isinstance(x,float) else float('nan')],60+j,-1)
P=np.array(P);X=np.array(X,dtype=float)
np.savez('alldata.npz',p=P,x=X,src=np.array(SRC),win=np.array(WIN))
print(len(P), np.isnan(X).sum())
