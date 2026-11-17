import numpy as np
d=np.load('alldata.npz'); p=d['p']; x=d['x']; src=d['src']; win=d['win']
q=p-0.5
keep=(q>=-0.425)&(p<=15/16)&np.isfinite(x)&(q!=0)
p,x,q,src,win=p[keep],x[keep],q[keep],src[keep],win[keep]
t=2*q; sg=np.sign(t); t=np.abs(t); x=x*sg
# dedupe on t (function of t only); keep first
o=np.lexsort((src,t)); t,x,src,win,p=t[o],x[o],src[o],win[o],p[o]
u=np.ones(len(t),bool); u[1:]=t[1:]!=t[:-1]
# check consistency among duplicates
dup=~u; bad=(x[1:]!=x[:-1])&(t[1:]==t[:-1])
print('dups',dup.sum(),'conflicting',bad.sum())
t,x,src,win,p=t[u],x[u],src[u],win[u],p[u]
print(len(t), 'A',(t*t<=0.5).sum())
t.tofile('t.bin'); x.tofile('x.bin'); np.savez('central.npz',t=t,x=x,src=src,win=win,p=p)
