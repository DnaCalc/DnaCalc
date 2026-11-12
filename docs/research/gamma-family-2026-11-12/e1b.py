# E1b: held-out inputs for the fitted B3 table (frozen with the table before Excel is asked).
import numpy as np, struct, json, hashlib, sys, random
sys.path.insert(0, "/home/koderbot/opus-lane")
import sim
def hx(v): return "0x"+struct.pack(">d", float(v)).hex()
def inputs():
    rng=random.Random(20261112)
    a=[2.0+rng.random() for _ in range(20000)]
    b=[2.0+j*2.0**-51 for j in range(-512,512) if j!=0]
    c=sim.window(3.0).tolist(); d=sim.window(1.625).tolist(); e=[3.0+5.0*rng.random() for _ in range(5000)]
    return [("rand",x) for x in a]+[("across2",x) for x in b]+[("w3",x) for x in c]+[("w1625",x) for x in d]+[("rec38",x) for x in e]
if __name__=="__main__":
    import predict
    rows=inputs(); out=predict.predict("GAMMALN",[[hx(r[1])] for r in rows],tag="e1b")
    open(sys.argv[1],"w").write("".join("%s\t%s\t%s\n"%(r[0],hx(r[1]),o) for r,o in zip(rows,out)))
    print(len(rows), hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())
