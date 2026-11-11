# E1 (answer 9 B.3, GAMMALN B3 band): windows of 1,024 consecutive doubles across [2,3].
import sim, numpy as np, hashlib, sys
def centres():
    cs=[2+(i+0.5)/256 for i in range(256)]
    cs+= [2.5, 2.75]
    return cs
def inputs():
    xs=np.concatenate([sim.window(c) for c in centres()])
    return xs
if __name__=="__main__":
    xs=inputs()
    P,Q=sim.f(sim.B3_P),sim.f(sim.B3_Q)
    lines=[]
    for i,x in enumerate(xs):
        pass
    preds={m: sim.b3(P,Q,xs,m) for m in (0,1,2)}
    with open("e1-predictions.tsv","w") as fh:
        fh.write("x\tH0_mode0\tH0_mode1\tH0_mode2\n")
        for i in range(len(xs)):
            fh.write("%s\t%s\t%s\t%s\n"%(sim.hx(xs[i]),sim.hx(preds[0][i]),sim.hx(preds[1][i]),sim.hx(preds[2][i])))
    print(len(xs), hashlib.sha256(open("e1-predictions.tsv","rb").read()).hexdigest())
