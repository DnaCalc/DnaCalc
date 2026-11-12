import numpy as np
def load(pred="e1-predictions.tsv", exc="e1-excel.tsv"):
    xs=[];E=[]
    for l in open(exc):
        a,b=l.split(); xs.append(int(a,16)); E.append(int(b,16))
    return np.array(xs,dtype=np.uint64).view(np.float64), np.array(E,dtype=np.uint64).view(np.float64)
