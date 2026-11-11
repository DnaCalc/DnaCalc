import json, collections, sys
bank=json.load(open(sys.argv[1])); rep=json.load(open(sys.argv[2])); corp=json.load(open(sys.argv[3]))
owners=json.load(open(sys.argv[4]))
rows_by={f["function"]:f for f in rep["functions"]}
S=bank["stats"]; worst=bank["worst"]
unread={"POISSON.DIST":4}
C=collections.defaultdict(collections.Counter)
for key,lst in corp["root_causes"].items():
    for c in lst:
        if key=="refusals":
            f=c["reason"].replace("not-modelled ",""); C[f]["refused"]+=c["cells"]; continue
        if key!="mismatches": continue
        f=c["function"]; s=c["shape"]
        if s.startswith("number, 1 ulp"): C[f]["1"]+=c["cells"]
        elif s.startswith("number, 2 to 16"): C[f]["2-16"]+=c["cells"]
        elif s.startswith("number,"): C[f][">16"]+=c["cells"]
        else: C[f]["nonnum"]+=c["cells"]
funcs=set()
for f,s in S.items():
    if s.get("gt4",0)+s.get("nonnum",0)+s.get("refused",0)+unread.get(f,0)>0: funcs.add(f)
for f,c in C.items():
    if c[">16"]+c["nonnum"]+c["refused"]+c["2-16"]>0: funcs.add(f)
def weight(f):
    s=S.get(f,{}); c=C.get(f,collections.Counter())
    return s.get("gt4",0)+s.get("nonnum",0)+s.get("refused",0)+unread.get(f,0)+c[">16"]+c["nonnum"]+c["refused"]
out=[]; tot_rows=0; amb=[]
for f in sorted(funcs,key=lambda f:(-weight(f),f)):
    s=S.get(f,{}); c=C.get(f,collections.Counter()); r=rows_by.get(f)
    w=weight(f)
    if w==0: amb.append(f); continue
    tot_rows+=w
    nn=s.get("nonnum",0)+s.get("refused",0)+unread.get(f,0)
    out.append("| %s | %s | %s | %s | %s | %s | %s | %s |"%(f, r["rows"] if r else "-", r["failed"] if r else "-", s.get("gt4",0), nn, worst.get(f,"-") if s.get("gt4") else "-",
        "%d / %d / %d"%(c[">16"],c["2-16"],c["nonnum"]+c["refused"]) if c else "-", owners.get(f,"**none**")))
print(len(out), tot_rows, "ambiguous:", amb)
open(sys.argv[5],"w").write("\n".join(out)+"\n\nAMB:"+", ".join(amb)+"\n")
