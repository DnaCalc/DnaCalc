import json, sys, collections, struct
def ord_key(b):
    # map double bits to a monotone integer line
    b=int(b,16)
    return b if b < (1<<63) else -(b-(1<<63))
def is_num(s): return isinstance(s,str) and s.startswith("0x") and len(s)==18
def ulp(e,a):
    ie,ia=int(e,16),int(a,16)
    exp_e=(ie>>52)&0x7ff; exp_a=(ia>>52)&0x7ff
    if exp_e==0x7ff or exp_a==0x7ff: return None
    return abs(ord_key(e)-ord_key(a))
stats=collections.defaultdict(lambda: collections.Counter())
worst=collections.defaultdict(int)
examples=collections.defaultdict(list)
for line in open(sys.argv[1]):
    r=json.loads(line)
    e,a=r["expected"],r["actual"]
    if e==a: stats[r["function"]]["match"]+=1; continue
    f=r["function"]; s=stats[f]; s["fail"]+=1
    if is_num(e) and is_num(a):
        u=ulp(e,a)
        if u is None: s["nonnum"]+=1; kind="nonnum"
        elif u<=4: s["le4"]+=1; kind=None
        else: s["gt4"]+=1; worst[f]=max(worst[f],u); kind="gt4"
    else:
        kind="refused" if ("refus" in str(a).lower() or "not" in str(a).lower()) else "nonnum"
        s[kind]+=1
    if kind and len(examples[f])<3: examples[f].append((r["args"],e,a,r["id"]))
out={"stats":{f:dict(c) for f,c in stats.items()},"worst":worst,"examples":examples}
json.dump(out,open(sys.argv[2],"w"))
