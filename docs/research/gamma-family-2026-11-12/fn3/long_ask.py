import ask, struct, json, sys, time
def b(x): return struct.unpack("<Q", struct.pack("<d", x))[0]
def f(u): return struct.unpack("<d", struct.pack("<Q", u))[0]
func=sys.argv[1]; centers=[float(c) for c in sys.argv[2].split(',')]; N=int(sys.argv[3]); out=sys.argv[4]
xs=[]
for c in centers:
    cb=b(c); xs += [f(cb+j) for j in range(-N//2, N//2)]
t=time.time(); g=ask.ask(func,[[x] for x in xs]); print(func, len(xs), time.time()-t, sum(1 for l,v in g if l!='n'))
with open(out,'w') as o:
    for x,(l,v) in zip(xs,g): o.write(f"{ask.hx(x)} {v}\n")
