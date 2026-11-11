import ask, time
for name,fn,col in (("e2","GAMMA",None),("e3","GAMMA.DIST",None)):
    rows=[l.rstrip("\n").split("\t") for l in open(name+"-predictions.tsv")][1:]
    if name=="e2": args=[[r[3][2:]] for r in rows]
    else: args=[[a[2:] for a in r[2:6]] for r in rows]
    t=time.time(); res,meta=ask.ask_bits(fn,args); print(name,len(res),"secs %.1f"%(time.time()-t),meta)
    with open(name+"-excel.tsv","w") as fh:
        for r in res: fh.write(("0x"+r["bits"]) if r["type"]=="number" else (r["type"]+":"+str(r.get("value",r.get("reason","")))) ); fh.write("\n")
