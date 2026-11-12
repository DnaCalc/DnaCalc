# E3 (answer 9 B.1): the lower-series mass from the density; the 1.1 mean seam; power-of-two scales.
import numpy as np, struct, json, hashlib, sys, math
sys.path.insert(0, "/home/koderbot/opus-lane")
def hx(v): return "0x"+struct.pack(">d", float(v)).hex()
def fl(h): return struct.unpack(">d", bytes.fromhex(h[2:]))[0]
def window(c, half=512):
    b=np.array([c]).view(np.int64)[0]; return np.arange(b-half,b+half,dtype=np.int64).view(np.float64)
ONE,ZERO="0x3ff0000000000000","0x0000000000000000"
def inputs():
    rows=[]
    for a0 in (0.75,1.75,2.75,5.5):
        for a in window(a0):
            for e in (-80,-96,-112):
                x=math.ldexp(a,e)
                for cum in (ONE,ZERO): rows.append(("a",e,[hx(x),hx(a),ONE,cum]))
    for a in (2.75,5.5):
        for x in window(1.1):
            for cum in (ONE,ZERO): rows.append(("m",0,[hx(x),hx(a),ONE,cum]))
    for args in json.load(open("density_fail.json")):
        y=fl(args[0])
        for b in (0.5,1.0,2.0,4.0):
            rows.append(("s",b,[hx(b*y),args[1],hx(b),ZERO]))
    return rows
if __name__=="__main__":
    import predict
    rows=inputs()
    ours=predict.predict("GAMMA.DIST",[r[2] for r in rows],tag="e3")
    with open("e3-predictions.tsv","w") as fh:
        fh.write("set\tparam\tx\ta\tb\tcum\tours\n")
        for r,o in zip(rows,ours): fh.write("%s\t%s\t%s\t%s\n"%(r[0],r[1],"\t".join(r[2]),o))
    print(len(rows), hashlib.sha256(open("e3-predictions.tsv","rb").read()).hexdigest())
