import sim, numpy as np, math
P,Q=sim.f(sim.B3_P),sim.f(sim.B3_Q)
xs=np.linspace(2,3,200001)
def stages(c,x):
    out=[]; v=c[8]
    for k in range(7,0,-1):
        p=v*x; out.append(("prod",k+1,p)); v=p+c[k]; out.append(("sum",k,v))
    p=v*x; out.append(("prod",1,p)); out.append(("sum",0,p+c[0]))
    return out
for name,c in (("P",P),("Q",Q)):
    st=stages(c,xs)
    for kind,k,val in st:
        e=np.floor(np.log2(np.abs(val)))
        ch=np.nonzero(np.diff(e))[0]
        for i in ch:
            print(name,kind,k,"%.6f"%xs[i], "2^%d->2^%d"%(e[i],e[i+1]), "sign" , np.sign(val[i]))
