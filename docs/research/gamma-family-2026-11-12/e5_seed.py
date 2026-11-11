# E5: the GAMMA seed Gamma(1+y) on [1,2): 256 windows of 1,024 consecutive doubles.
import numpy as np, struct, hashlib
SEED_P=[-1.71618513886549492533811e0,2.47656508055759199108314e1,-3.79804256470945635097577e2,6.29331155312818442661052e2,8.66966202790413211295064e2,-3.14512729688483675254357e4,-3.61444134186911729807069e4,6.64561438202405440627855e4]
SEED_Q=[-3.08402300119738975254353e1,3.15350626979604161529144e2,-1.01515636749021914166146e3,-3.10777167157231109440444e3,2.25381184209801510330112e4,4.75584627752788110767815e3,-1.34659959864969306392456e5,-1.15132259675553483497211e5]
def ours(x):
    y=x-1.0; num=0.0; den=1.0
    for i in range(8): num=(num+SEED_P[i])*y; den=den*y+SEED_Q[i]
    return num/den+1.0
def hx(v): return "0x"+struct.pack(">d", float(v)).hex()
def inputs():
    out=[]
    for i in range(256):
        c=np.array([1+(i+0.5)/256]).view(np.int64)[0]
        out.extend(np.arange(c-512,c+512,dtype=np.int64).view(np.float64).tolist())
    return out
if __name__=="__main__":
    xs=inputs()
    with open("e5-predictions.tsv","w") as fh:
        fh.write("x\tours\n")
        for x in xs: fh.write("%s\t%s\n"%(hx(x),hx(ours(x))))
    print(len(xs), hashlib.sha256(open("e5-predictions.tsv","rb").read()).hexdigest())
