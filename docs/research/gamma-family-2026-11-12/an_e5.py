import numpy as np, struct, hpgamma, math
from decimal import Decimal as D
from fractions import Fraction as Fr
X=[l.split("\t") for l in open("e5-predictions.tsv").read().splitlines()[1:]]
E=[l.strip() for l in open("e5-excel.tsv")]
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
xs=np.array([fl(a) for a,_ in X]); ours=np.array([fl(b) for _,b in X]); ex=np.array([fl(e) for e in E])
print("ours match", (ours==ex).mean())
W=256
# truth at window centres only (window spans 2.3e-13 of y: truth varies by ~1e-13 relative -> need per point? use linear interp)
res=[]
for w in range(W):
    seg=slice(w*1024,(w+1)*1024)
    x0=xs[w*1024+512]; g0=hpgamma.gamma_hp(x0); g1=hpgamma.gamma_hp(xs[w*1024+1023])
    xd0=Fr(x0); xd1=Fr(xs[w*1024+1023])
    slope=(g1-g0)/D((xd1-xd0).numerator)*D((xd1-xd0).denominator)
    u=math.ulp(ex[w*1024+512])
    tr=[float((D(Fr(x).numerator)/D(Fr(x).denominator)-D(xd0.numerator)/D(xd0.denominator))*slope+g0) for x in xs[seg]]
    # error in ulps computed in decimal for accuracy
    errs=[float((D(Fr(e).numerator)/D(Fr(e).denominator) - ((D(Fr(x).numerator)/D(Fr(x).denominator)-D(xd0.numerator)/D(xd0.denominator))*slope+g0))/D(Fr(u).numerator)*D(Fr(u).denominator)) for x,e in zip(xs[seg],ex[seg])]
    errs=np.array(errs)
    res.append((x0, errs.mean(), errs.std(), (ours[seg]==ex[seg]).mean()))
np.save("e5_means.npy",np.array(res))
for r in res[::4]: print("y %.4f mean(E-true) %+.3f ulp  sd %.3f  ours-match %.2f"%(r[0]-1,r[1],r[2],r[3]))
