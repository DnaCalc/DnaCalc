# E2 (answer 9 B.4, with the half anchors -0.5 .. -3.5): GAMMA(n + y) at exact translations.
import numpy as np, struct, json, hashlib, sys
sys.path.insert(0, "/home/koderbot/opus-lane")
def hx(v): return "0x"+struct.pack(">d", float(v)).hex()
def inputs():
    Y1=[0.25+j*2.0**-49 for j in range(1024)]
    Y2=[0.5+j*2.0**-49 for j in range(-512,512)]
    rows=[]
    for tag,Y in (("q",Y1),("h",Y2)):
        for y in Y:
            for n in range(-9,10):
                x=n+y; assert x-n==y
                rows.append((tag,n,y,x))
    return rows
if __name__=="__main__":
    import predict
    rows=inputs()
    ours=predict.predict("GAMMA",[[hx(r[3])] for r in rows],tag="e2")
    with open("e2-predictions.tsv","w") as fh:
        fh.write("set\tn\ty\tx\tours\n")
        for r,o in zip(rows,ours): fh.write("%s\t%d\t%s\t%s\t%s\n"%(r[0],r[1],hx(r[2]),hx(r[3]),o))
    print(len(rows), hashlib.sha256(open("e2-predictions.tsv","rb").read()).hexdigest())
