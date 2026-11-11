"""Our current calc-func outputs for arbitrary rows, through the full-bank judge on a private evidence root."""
import json, subprocess, os, sys, copy
ROOT=os.path.expanduser("~/opus-lane/legacy")
EV=os.path.expanduser("~/projects/DnaCalc/CalcFunc-Evidence/readings/excel-20527/COS.json")
CLONE=os.path.expanduser("~/projects/DnaCalc/DnaCalc-opus")
def predict(function, rows, expected=None, tag="p"):
    prov=json.load(open(EV))["capture_provenance"]
    prov=copy.deepcopy(prov)
    ws=[{"id":"%s-%d"%(tag,i),"args":list(a),"expected_bits":(expected[i] if expected else "0x0000000000000000")} for i,a in enumerate(rows)]
    d=os.path.join(ROOT,"CalcFunc-Evidence","readings","excel-20527")
    for f in os.listdir(d): os.remove(os.path.join(d,f))
    json.dump({"capture_provenance":prov,"function":function,"witnesses":ws},open(os.path.join(d,function+".json"),"w"))
    env=dict(os.environ, DNACALC_LEGACY_ROOT=ROOT, PATH=os.path.expanduser("~/.cargo/bin")+":"+os.environ["PATH"], CARGO_PROFILE_DEV_DEBUG="0", CARGO_BUILD_JOBS="6")
    tr=os.path.expanduser("~/opus-lane/pred-trace.jsonl")
    p=subprocess.run(["cargo","xtask","witness","judge","--bank","full","--func",function,"--report",os.path.expanduser("~/opus-lane/pred-report.json"),"--trace",tr],cwd=CLONE,env=env,capture_output=True,text=True)
    if p.returncode not in (0,1): print(p.stderr[-3000:]); raise SystemExit(p.returncode)
    out={}
    for l in open(tr):
        r=json.loads(l); out[int(r["id"].split("-")[-1])]=r["actual"]
    return [out.get(i) for i in range(len(rows))]
if __name__=="__main__":
    print(predict("GAMMALN",[["0x4004000000000000"],["0x4006000000000000"]]))
