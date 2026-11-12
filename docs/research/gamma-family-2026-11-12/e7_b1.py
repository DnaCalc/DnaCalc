# E7: GAMMALN on the B1 band [0.7, 1.5): 256 windows of 1,024 consecutive doubles.
import numpy as np, struct, hashlib, sys
sys.path.insert(0, "/home/koderbot/opus-lane")
def hx(v): return "0x"+struct.pack(">d", float(v)).hex()
def inputs():
    out=[]
    for i in range(256):
        c=np.array([0.7+0.8*(i+0.5)/256]).view(np.int64)[0]
        out.extend(np.arange(c-512,c+512,dtype=np.int64).view(np.float64).tolist())
    return out
if __name__=="__main__":
    import predict
    xs=inputs(); ours=predict.predict("GAMMALN",[[hx(x)] for x in xs],tag="e7")
    with open("e7-predictions.tsv","w") as fh:
        fh.write("x\tours\n")
        for x,o in zip(xs,ours): fh.write("%s\t%s\n"%(hx(x),o))
    print(len(xs), hashlib.sha256(open("e7-predictions.tsv","rb").read()).hexdigest())
