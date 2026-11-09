#!/usr/bin/env python3
"""Writes specs/*.json for volatility-job.ps1 (vol-1). Run: py -3 make-specs.py [name ...]"""
import json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
XLL = r"C:\Work\DnaCalc\codex-runs\vol-1\work\xll\calc_xll_fixture.xll"
CONSTS = [("C1", 1), ("C2", 2), ("C3", 3), ("D1", 10), ("D2", 20), ("D3", 30), ("D4", 40)]

def standard(formulas, consts=CONSTS, trials=3, clean=3, warm=2, id=None, names=(), xcfg=(), vcfg=(),
             pre_steps=(), post_steps=(), extra_cells=(), final_save=True, extra_consts=(), stride=1):
    """Each formula in A<i>; its dependent counter =Cnt(A<i>&"") in B<i>. (A bare =Cnt(A<i>) is not
    recalculated by a volatile precedent, instr probe; the concatenation form is.)"""
    cells, watch = [], []
    for i, f in enumerate(formulas, 1):
        r = (i - 1) * stride + 1
        cells.append([f"A{r}", f])
        cells.append([f"B{r}", f'=Cnt(A{r}&"")'])
        watch.append(f"A{r}")
    cells += [list(c) for c in extra_cells]
    steps = list(pre_steps)
    for w in range(warm):
        steps.append({"op": "calc", "tag": f"warm{w+1}"})
    for k in range(clean):
        steps.append({"op": "calc", "tag": f"clean{k+1}"})
    if final_save:
        steps.append({"op": "save", "tag": "final"})
    steps += list(post_steps)
    return {"id": id, "trials": trials, "xll_cfg": [list(x) for x in xcfg], "vba_cfg": [list(x) for x in vcfg],
            "consts": [list(c) for c in list(consts) + list(extra_consts)], "names": [list(n) for n in names], "cells": cells,
            "watch": watch, "steps": steps}

SPECS = {}
def spec(name, probes, xll=False):
    SPECS[name] = {"xll": XLL if xll else None, "probes": probes}

# ---- instrument calibration: twelve formulas of known behaviour, three clean Calculates ----
CAL = ["=RAND()", "=NOW()*0", "=1+2", "=SUM(C1:C3)", "=VTrue()", "=VPlain()", "=VFalse()", "=VOnceTrue()",
       "=IF(TRUE,1,RAND())", "=OFFSET(C1,0,0)", "=INDIRECT(\"C1\")", "=TODAY()*0"]
spec("cal1", [standard(CAL, id="cal1", post_steps=[{"op": "savexlsx", "tag": "final"}])])

# ---- the counter form: a bare reference argument is not recalculated by a volatile precedent ----
cells = [["A1", "=RAND()"], ["A2", "=IF(TRUE,1,RAND())"], ["A3", "=C9*0"], ["A4", "=C9+0"], ["A5", "=NOW()*0"]]
for i in range(1, 6):
    cells += [[f"B{i}", f"=Cnt(A{i})"], [f"C{i}", f'=Cnt(A{i}&"")'], [f"D{i}", f"=Cnt(A{i}+0)"]]
spec("instr", [{"id": "instr", "trials": 3, "consts": [["C9", 1]], "cells": cells, "watch": ["A1", "A3", "A4"],
    "steps": [{"op": "calc", "tag": "warm1"}, {"op": "calc", "tag": "warm2"}, {"op": "calc", "tag": "clean1"}, {"op": "calc", "tag": "clean2"},
              {"op": "set", "addr": "C9", "value": 2}, {"op": "calc", "tag": "edit1"}, {"op": "calc", "tag": "clean3"}, {"op": "calc", "tag": "clean4"},
              {"op": "save", "tag": "final"}]}])

# ---- P1: syntactic layer of built-ins ----
P1A = [  # each volatile built-in, and controls
    "=RAND()", "=RANDBETWEEN(1,10)", "=SUM(RANDARRAY(1,1))", "=NOW()", "=TODAY()", "=OFFSET(C1,0,0)", "=INDIRECT(\"C1\")",
    "=INDIRECT(\"R1C3\",FALSE)", "=SUM(OFFSET(C1,0,0,2,1))", "=SUM(INDIRECT(\"C1:C2\"))",
    "=1+2", "=SUM(C1:C3)", "=ROW()", "=ROW(C1)", "=COLUMN()", "=ROWS(C1:C3)", "=COLUMNS(C1:C3)", "=AREAS(C1:C3)",
    "=AREAS((C1,C2))", "=INDEX(C1:C3,2)", "=SUM(INDEX(C1:C3,2))", "=SUM(C1:INDEX(C1:C3,2))", "=SUM(INDEX(C1:C3,2):C3)",
    "=SUM(C1:INDEX(C:C,3))", "=ISREF(C1)", "=ISFORMULA(C1)", "=FORMULATEXT(A1)", "=SHEET()", "=SHEETS()",
    "=SUBTOTAL(9,C1:C3)", "=AGGREGATE(9,0,C1:C3)", "=N(C1)", "=T(C1)", "=ADDRESS(1,1)", "=CHOOSE(2,C1,C2)",
    "=SUM(CHOOSE({1,2},C1,C2))", "=MATCH(2,C1:C3,0)", "=VLOOKUP(2,C1:D3,2,FALSE)", "=XLOOKUP(2,C1:C3,D1:D3)",
    "=HYPERLINK(\"x\",\"y\")", "=TEXT(1,\"0\")", "=DATE(2020,1,1)", "=EOMONTH(DATE(2020,1,1),1)", "=WORKDAY(DATE(2020,1,1),1)",
    "=ISBLANK(C9)", "=INDEX(C1:C3,0)", "=SUM(INDEX(C1:D3,0,1))", "=ROW(INDIRECT(\"C1\"))", "=SUM(TRANSPOSE(C1:C3))",
    "=SUMPRODUCT(C1:C3,D1:D3)", "=LOOKUP(2,C1:C3,D1:D3)", "=STDEV(C1:C3)", "=SUM(IF(C1:C3>1,1,0))",
]
P1B = [  # untaken branches, LET/LAMBDA, laziness
    "=IF(TRUE,1,RAND())", "=IF(FALSE,RAND(),1)", "=IF(C2=2,1,RAND())", "=CHOOSE(1,1,NOW())", "=CHOOSE(2,NOW(),1)",
    "=IF(TRUE,1,INDIRECT(\"C1\"))", "=IF(TRUE,1,OFFSET(C1,0,0))", "=IFERROR(1,RAND())", "=IFNA(1,RAND())",
    "=IFS(TRUE,1,TRUE,RAND())", "=SWITCH(1,1,1,2,RAND())", "=AND(FALSE,RAND()>0.5)", "=OR(TRUE,RAND()>0.5)",
    "=LET(f,LAMBDA(RAND()),1)", "=LET(x,RAND(),1)", "=LET(x,RAND(),x*0)", "=LET(x,1,IF(TRUE,x,RAND()))",
    "=(LAMBDA(x,1))(RAND())", "=SUM(MAP(C1:C3,LAMBDA(x,x*RAND())))", "=SUM(MAP(C1:C3,LAMBDA(x,x)))",
    "=LET(f,LAMBDA(x,x+RAND()),1)", "=LET(f,LAMBDA(x,x+RAND()),f(1)*0)", "=IF(TRUE,1,SUM(RAND()))",
    "=IF(TRUE,1,IF(TRUE,1,RAND()))", "=1*IF(TRUE,1,RAND())", "=SUM(1,IF(TRUE,1,RAND()))", "=IF(ISNUMBER(C1),1,NOW())",
    "=IF(C1=1,TODAY(),0)*0", "=IF(C1=2,TODAY(),0)*0", "=ISNUMBER(RAND())", "=N(RAND())*0",
    "=IF(TRUE,SUM(C1:C3),OFFSET(C1,0,0))", "=IF(FALSE,1,SUM(OFFSET(C1,0,0)))", "=COUNT(RAND())*0+1",
]
P1C = []  # CELL and INFO, literal type and type from a cell
CELL_T = ["address", "col", "color", "contents", "filename", "format", "parentheses", "prefix", "protect", "row", "type", "width"]
INFO_T = ["directory", "numfile", "origin", "osversion", "recalc", "release", "system", "memavail", "memused", "totmem"]
EXTRA = []   # constants holding the type text, column F (cell) and G (info)
for i, t in enumerate(CELL_T, 1):
    EXTRA.append((f"F{i}", t))
    P1C.append(f'=CELL("{t}",C1)')
    P1C.append(f"=CELL(F{i},C1)")
    P1C.append(f'=CELL("{t}")')
for i, t in enumerate(INFO_T, 1):
    EXTRA.append((f"G{i}", t))
    P1C.append(f'=INFO("{t}")')
    P1C.append(f"=INFO(G{i})")
P1C += ['=CELL("address",C1:C3)', '=CELL("row",INDEX(C1:C3,2))', '=CELL("row",OFFSET(C1,1,0))', '=CELL("contents",C1)&""',
        '=CELL("format",C1)&""', '=CELL("nosuch",C1)', '=INFO("nosuch")', '=CELL("ADDRESS",C1)', '=CELL(" address",C1)', '=CELL("addr",C1)']
P1D = [  # SUMIF family, matched and mismatched shapes
    '=SUMIF(C1:C3,">1",D1:D3)', '=SUMIF(C1:C3,">1",D1:D2)', '=SUMIF(C1:C3,">1",D1:E3)', '=SUMIF(C1:C3,">1",D1)',
    '=SUMIF(C1:C3,">1")', '=SUMIF(C1:C3,">1",D2:D4)', '=SUMIF(C1:C3,">1",D1:D3)+0', '=SUMIF(C1:D3,">1",D1:E3)',
    '=COUNTIF(C1:C3,">1")', '=AVERAGEIF(C1:C3,">1",D1:D3)', '=AVERAGEIF(C1:C3,">1",D1:D2)', '=AVERAGEIF(C1:C3,">1",D1)',
    '=SUMIFS(D1:D3,C1:C3,">1")', '=SUMIFS(D1:D2,C1:C3,">1")', '=COUNTIFS(C1:C3,">1",D1:D3,">10")', '=COUNTIFS(C1:C3,">1",D1:D2,">10")',
    '=SUMIF(C1:C3,">1",D1:D3)*0', '=SUMIF(C1:C3,">1",D1:D1)', '=SUMIF(C1,">1",D1:D3)', '=SUMIF(C1:C1,">1",D1:D3)',
    '=SUMIF(INDEX(C:C,1):INDEX(C:C,3),">1",D1:D3)', '=SUMIF(C1:C3,">1",INDEX(D:D,1):INDEX(D:D,2))',
    '=MAXIFS(D1:D3,C1:C3,">1")', '=MAXIFS(D1:D2,C1:C3,">1")', '=MINIFS(D1:D3,C1:C3,">1")',
]
spec("p1a", [standard(P1A, id="p1a")])
spec("p1b", [standard(P1B, id="p1b")])
# CELL/INFO in groups, each its own job (a dialog in one group must not block the others)
p1c_groups = {
    "p1ca": [f'=CELL("{t}",C1)' for t in CELL_T],
    "p1cb": [f"=CELL(F{i},C1)" for i in range(1, len(CELL_T) + 1)],
    "p1cc": [f'=CELL("{t}")' for t in CELL_T],
    "p1cd": [f'=INFO("{t}")' for t in INFO_T],
    "p1ce": [f"=INFO(G{i})" for i in range(1, len(INFO_T) + 1)],
    "p1cf": P1C[-10:],
}
for _name, _formulas in p1c_groups.items():
    spec(_name, [standard(_formulas, id=_name, extra_consts=EXTRA)])
spec("p1d", [standard(P1D, id="p1d")])
# spills: free and blocked, array-producing functions (stride 8 leaves room to spill)
P1E = ["=SEQUENCE(3)", "=C1:C3", "=INDEX(C1:C3,0)", "=TRANSPOSE(C1:C3)", "=C1:C3*2", "=SORT(C1:C3)", "=FILTER(C1:C3,C1:C3>1)", "=SEQUENCE(3)",
       "=SEQUENCE(3)+RAND()*0", "=RANDARRAY(3)", "=OFFSET(C1,0,0,3,1)", "=INDIRECT(\"C1:C3\")", "=TEXTSPLIT(\"a,b\",\",\")", "=EXPAND(C1:C2,3,1)", "=@C1:C3", "=SEQUENCE(3)", "=SEQUENCE(1)"]
p1e = standard(P1E, id="p1e", stride=8)
# a blocker in the spill range of the eighth formula (A57:A59): constant in A58
p1e["consts"].append(["A58", 99])
spec("p1e", [p1e])

# ---- P2: one flag or two (F1, F2, F3): a built-in and a user function that requests, both orders ----
P2 = [
    "=RAND()+VFalse()", "=VFalse()+RAND()", "=IF(TRUE,VFalse(),RAND())", "=IF(TRUE,RAND(),VFalse())",
    '=INDIRECT("C1")+VFalse()', '=VFalse()+INDIRECT("C1")', "=NOW()+VTF()", "=NOW()+VFT()", "=VTF()+NOW()", "=VFT()+NOW()",
    "=RAND()+VTrue()", "=VTrue()+RAND()", "=VTrue()+1", "=1+VTrue()", "=VTrue()+VFalse()", "=VFalse()+VTrue()",
    "=RAND()*0+VPlain()", "=VPlain()+RAND()", "=SUM(RAND(),VFalse())", "=SUM(VFalse(),RAND())", "=VFalse()+IF(TRUE,1,RAND())",
    "=IF(TRUE,1,RAND())+VFalse()", "=VArgF(RAND())", "=VArgF(IF(TRUE,1,RAND()))", "=VArgP(RAND())", "=VArgT(1)",
    "=VFalse()+0*VFalse()", "=VFalse()", "=VTrue()", "=VPlain()", "=VFalse()+VPlain()", "=VPlain()+VFalse()",
    "=VTrue()+VPlain()", "=VPlain()+VTrue()", "=VNoArg()", "=VHelper()", "=VFalse()+VHelper()", "=VHelper()+VFalse()",
    "=VSeq()", "=VOnceTrue()", "=VOnceFalse()", "=VTTF()", "=IF(VFalse()=1,RAND(),0)", "=IF(RAND()>2,0,VFalse())",
]
P2X = [
    "=RAND()+XFalse()", "=XFalse()+RAND()", '=INDIRECT("C1")+XFalse()', '=XFalse()+INDIRECT("C1")', "=NOW()+XTF()", "=XTF()+NOW()",
    "=NOW()+XFT()", "=XFT()+NOW()", "=RAND()+XTrue()", "=XTrue()+RAND()", "=XTrue()", "=XFalse()", "=XPlain()", "=XTF()", "=XFT()",
    "=XTTF()", "=XSeq()", "=XNoArg()", "=XFalse()+XTrue()", "=XTrue()+XFalse()", "=XPlain()+RAND()", "=RAND()+XPlain()",
    "=XFalse()+VFalse()", "=VFalse()+XFalse()", "=XTrue()+VFalse()", "=VFalse()+XTrue()", "=XFalse()+VTrue()", "=VTrue()+XFalse()",
    "=IF(TRUE,XFalse(),RAND())",
]
spec("p2", [standard(P2, id="p2")])
spec("p2x", [standard(P2X, id="p2x")], xll=True)

# ---- P3: registration '!' : default timing and clearability; status codes per registration class ----
P3 = [
    "=Bang()", "=BangClear()", "=BangClearOnce()", "=BangTrue()", "=BangFT()", "=BangSeq()",
    "=Bang()+VPlain()", "=VPlain()+Bang()", "=Bang()+VFalse()", "=VFalse()+Bang()", "=BangClear()+Bang()", "=Bang()+BangClear()",
    "=Bang()+XFalse()", "=XFalse()+Bang()", "=Bang()+XTrue()", "=Bang()+XPlain()", "=XPlain()+Bang()", "=BangClear()+XTrue()", "=XTrue()+BangClear()",
    "=BangClear()+RAND()", "=RAND()+BangClear()", "=IF(TRUE,BangClear(),RAND())", "=IF(TRUE,1,Bang())", "=IF(FALSE,Bang(),1)",
    "=BangClear()+0*BangClear()", "=Bang()*0+BangClear()", "=XFalse()", "=HashFalse()", "=DollarF()", "=XTrue()", "=HashTrue()", "=DollarT()",
    "=Hash()", "=Dollar()", "=XPlain()", "=BangClear()+VFalse()", "=VFalse()+BangClear()",
    "=BangClearOnce()+VPlain()", "=BangFT()+VPlain()", "=VPlain()+BangFT()",
]
spec("p3", [standard(P3, id="p3", clean=4)], xll=True)

# ---- P4: '#' and argument types: registration text alone, or a reference actually passed ----
P4 = []
for f in ["Hu", "Hq", "Hr", "Hp", "Gu", "Gq", "Gr", "Gp", "HuC", "HqC", "HuF", "HuT", "GuC", "GuF", "GuT", "BangHu", "BangHuF", "BangGu", "DollarU", "DollarUT"]:
    P4 += [f"={f}(C1)", f"={f}(1)", f"={f}(C1:C2)", f"={f}(C1+0)", f"={f}(INDEX(C1:C3,2))", f"={f}(Z99)"]
for f in ["Hb", "Gb"]:
    P4 += [f"={f}(C1)", f"={f}(1)"]
for f in ["Hub", "HubC", "Gub"]:
    P4 += [f"={f}(C1,1)", f"={f}(1,1)", f"={f}(1,C1)", f"={f}(C1,C2)"]
for f in ["Hbu", "HbuC"]:
    P4 += [f"={f}(1,C1)", f"={f}(1,1)", f"={f}(C1,1)", f"={f}(C1,C2)"]
for f in ["Hrr", "Huu"]:
    P4 += [f"={f}(C1,C2)", f"={f}(1,C2)", f"={f}(C1,1)", f"={f}(1,1)"]
P4 += ["=Hash()", "=Dollar()", "=HashTrue()", "=HashFalse()"]
spec("p4", [standard(P4, id="p4")], xll=True)

# ---- P5: names and untaken code ----
NAMES = [["NmRand", "=RAND()"], ["NmVba", "=VTrue()"], ["NmLam", "=LAMBDA(RAND())"], ["NmOne", "=1"], ["NmInd", "=INDIRECT(\"C1\")"],
         ["NmIf", "=IF(TRUE,1,RAND())"], ["NmVF", "=VFalse()"], ["NmX", "=C1+0"]]
P5 = ["=NmRand", "=IF(TRUE,1,NmRand)", "=IF(TRUE,1,NmVba)", "=NmVba", "=LET(f,NmLam,1)", "=NmLam()", "=NmOne", "=NmRand*0", "=NmInd", "=IF(FALSE,NmRand,1)",
      "=NmIf", "=NmVF", "=NmVF+RAND()", "=RAND()+NmVF", "=NmX", "=NmOne+0", "=NmVba+VFalse()", "=VFalse()+NmVba", "=IF(TRUE,1,NmLam())", "=LET(f,NmLam,f())*0"]
steps = [{"op": "calc", "tag": "warm1"}, {"op": "calc", "tag": "warm2"}, {"op": "calc", "tag": "s1c1"}, {"op": "calc", "tag": "s1c2"}, {"op": "save", "tag": "s1"},
         {"op": "name", "name": "NmRand", "refers": "=1"}, {"op": "name", "name": "NmVba", "refers": "=VPlain()"}, {"op": "name", "name": "NmLam", "refers": "=LAMBDA(1)"},
         {"op": "calc", "tag": "s2e"}, {"op": "calc", "tag": "s2c1"}, {"op": "calc", "tag": "s2c2"}, {"op": "save", "tag": "s2"},
         {"op": "name", "name": "NmRand", "refers": "=RAND()"}, {"op": "name", "name": "NmVba", "refers": "=VTrue()"}, {"op": "name", "name": "NmLam", "refers": "=LAMBDA(RAND())"},
         {"op": "calc", "tag": "s3e"}, {"op": "calc", "tag": "s3c1"}, {"op": "calc", "tag": "s3c2"}, {"op": "save", "tag": "s3"}]
p5 = standard(P5, id="p5", names=NAMES, final_save=False)
p5["steps"] = steps
spec("p5", [p5])

spec("scan", [{"id": "scan", "trials": 1, "cells": [["A1", "=VolScan(200,260)"], ["A2", "=VolScan(261,330)"], ["A3", "=VolScan(331,400)"]], "watch": ["A1", "A2", "A3"],
               "steps": [{"op": "calc", "tag": "c1"}]}], xll=True)

cand = [244, 255, 261, 283, 296, 322, 323, 330, 347, 352, 360, 366, 367, 369, 370, 371, 372, 373, 374, 375, 376, 377, 378, 380, 384, 385, 386, 387, 388, 389, 390, 391, 392, 393, 394, 395, 399, 400, 237, 258, 259, 260]
spec("scan2", [standard([f"=XNmOne({n})" for n in cand], id="scan2", trials=1, clean=2)], xll=True)

P2H = ["=RAND()+HashFalse()", "=HashFalse()+RAND()", "=NOW()+HTF()", "=NOW()+HFT()", "=HTF()+NOW()", "=HFT()+NOW()",
       "=HashTrue()+VFalse()", "=VFalse()+HashTrue()", "=HashFalse()+VTrue()", "=VTrue()+HashFalse()", "=HashFalse()+HashTrue()", "=HashTrue()+HashFalse()",
       "=BangHash()", "=BangHashClear()", "=BangHashClearOnce()", "=BangHashTrue()", "=BangHashFT()", "=BangHashSeq()",
       "=BangHashClear()+Bang()", "=Bang()+BangHashClear()", "=BangHashClear()+VPlain()", "=VPlain()+BangHashClear()",
       "=BangHashClear()+RAND()", "=RAND()+BangHashClear()", "=IF(TRUE,BangHashClear(),RAND())", "=BangHashClear()+BangHashClear()",
       "=HuF(C1)+Bang()", "=Bang()+HuF(C1)", "=Hu(C1)+HashFalse()", "=HashFalse()+Hu(C1)", "=Hu(C1)+VFalse()", "=VFalse()+Hu(C1)",
       "=HuF(C1)+Hu(C1)", "=Hu(C1)+HuF(C1)", "=HuTF(C1)", "=HuFT(C1)", "=HuFT(1)", "=HuTF(1)",
       "=HOnceF()", "=HOnceT()", "=HSeq()", "=HSeq()+HSeq()", "=HSeq()+HSeq()+HSeq()", "=HNoArg()", "=HTTF()", "=HTF()", "=HFT()",
       "=HuOnceF(C1)", "=HuSeq(C1)", "=HuSeq(1)", "=Hu(C1)+0*Hu(1)", "=BangHashSeq()+BangHashSeq()"]
spec("p2h", [standard(P2H, id="p2h", clean=4)], xll=True)

def C(tag): return {"op": "calc", "tag": tag}

# ---- X1: persistence across Calculate kinds (a function that stops requesting, then restarts) ----
x1_steps = [C("w1"), C("w2"), C("v1"), C("v2"), {"op": "save", "tag": "s_vol"},
            {"op": "vcfg", "name": "VSeq", "script": ""}, C("stop"), C("q1"), C("q2"), {"op": "save", "tag": "s_stopped"},
            {"op": "sheet", "tag": "q_sheet"}, C("q3"), {"op": "range", "addr": "A1", "tag": "q_range"}, C("q4"),
            {"op": "full", "tag": "q_full"}, C("q5"), C("q6"), {"op": "rebuild", "tag": "q_rebuild"}, C("q7"), C("q8"),
            {"op": "vcfg", "name": "VSeq", "script": "T"}, C("t0_nochange"), C("t0b"), {"op": "full", "tag": "t_full"}, C("r1"), C("r2"), {"op": "save", "tag": "s_restarted"},
            {"op": "vcfg", "name": "VSeq", "script": "F"}, C("f0"), C("f1"), C("f2"), {"op": "save", "tag": "s_end"},
            {"op": "auto", "tag": "to_auto"}, {"op": "set", "addr": "C5", "value": 7}, {"op": "read", "tag": "auto_edit_unrelated"}, {"op": "set", "addr": "C5", "value": 8}, {"op": "read", "tag": "auto_edit_unrelated2"}, {"op": "manual"}]
x1 = standard(["=VSeq()", "=RAND()", "=C1+0", "=VSeq()&\"\""], id="x1", warm=0, clean=0, final_save=False, vcfg=[("VSeq", "T")], post_steps=x1_steps)
spec("x1", [x1])

# ---- X2: lifted and repeated calls ----
X2 = ["=SUM(VElemD(C1:C3))", "=SUM(VElemD(E1:E3))", "=SUM(VElemD(TRANSPOSE(C1:C3)))", "=SUM(VElemD({1,2,3}))", "=SUM(VElemD({3,2,1}))",
      "=VElemD(2)+VElemD(1)", "=VElemD(1)+VElemD(2)", "=VTrue()+VFalse()+VTrue()", "=VTrue()+VFalse()+VFalse()", "=VFalse()+VTrue()+VTrue()+VFalse()",
      "=VTrue()+VTrue()", "=VFalse()+VFalse()", "=VTTF()", "=VTF()", "=SUM(VElemD(C1:D3))", "=SUM(VElemD(C1:C3))+VFalse()", "=VFalse()+SUM(VElemD(C1:C3))",
      "=MAP(C1:C3,LAMBDA(x,VElemD(x)))", "=SUM(MAP(C1:C3,LAMBDA(x,VElemD(x))))", "=SUM(MAP(E1:E3,LAMBDA(x,VElemD(x))))", "=SUM(BYROW(C1:C3,LAMBDA(r,VElemD(r))))"]
spec("x2", [standard(X2, id="x2", extra_consts=[("E1", 3), ("E2", 2), ("E3", 1)])])

x2b = standard(["=SUM(VElemD(F1:F3))", "=SUM(VElemD(F1))", "=VElemD(F1)", "=SUM(VElemD(C1:C3))", "=SUM(VElemD(G1:G3))", "=SUM(VElemD(H1:H3))", "=SUM(VElemD(H3:H1))"], id="x2b",
               extra_consts=[("F1", 5), ("F2", 6), ("F3", 7), ("G1", 3), ("G2", 3), ("G3", 1), ("H1", 1), ("H2", 3), ("H3", 3)])
spec("x2b", [x2b])

# ---- X3: threads and async (xlfVolatile from `$`, from async entries and after the async return) ----
X3 = ["=AsyncPlain(1)", "=AsyncT(1)", "=AsyncF(1)", "=AsyncBang(1)", "=AsyncBangF(1)", "=AsyncDollarT(1)", "=AsyncDollarF(1)", "=AsyncLate(1)",
      "=RAND()+AsyncF(1)", "=AsyncF(1)+RAND()", "=DollarT()", "=DollarF()", "=DollarUT(C1)", "=XTrue()"]
x3 = standard(X3, id="x3", clean=4)
spec("x3", [x3], xll=True)

# ---- X4: an abandoned attempt (three-level producer chain read by a cell entered before it) ----
x4 = standard(["=VAtOne()+E3", "=VAtTwo()+E3", "=VAtThree()+E3", "=VAtFour()+E3", "=VAtFive()+E3", "=VAtTwoB()+0"],
              id="x4", trials=1, warm=3, clean=1, consts=[("C1", 5)],
              extra_cells=[("E1", "=C1*2"), ("E2", "=E1+1"), ("E3", "=E2+1")], post_steps=[{"op": "set", "addr": "C1", "value": 6}, C("edit1"), C("e1"), C("e2")])
spec("x4", [x4])

# ---- X5: outside a worksheet call ----
x5 = standard(["=VPlain()", "=VTrue()"], id="x5", clean=2, post_steps=[
    {"op": "run", "macro": "OutsideVolatile", "tag": "run_function"}, {"op": "run", "macro": "SubVolatile", "tag": "run_sub"}, {"op": "run", "macro": "GetStatus", "tag": "sub_status"},
    {"op": "eval", "expr": "VTrue()", "tag": "evaluate_vtrue"}, {"op": "eval", "expr": "VFalse()", "tag": "evaluate_vfalse"}, {"op": "eval", "expr": "OutsideVolatile()", "tag": "evaluate_outside"},
    C("after1"), C("after2"), {"op": "save", "tag": "s_after"}])
spec("x5", [x5])
x5x = standard(["=HashTrue()", "=HashFalse()", "=XPlain()"], id="x5x", clean=2, post_steps=[
    {"op": "xrun", "name": "HashTrue", "tag": "xrun_hash_true"}, {"op": "xrun", "name": "HashFalse", "tag": "xrun_hash_false"}, {"op": "xrun", "name": "XTrue", "tag": "xrun_ordinary"},
    {"op": "eval", "expr": "HashTrue()", "tag": "evaluate_hash"}, C("after1"), C("after2")])
spec("x5x", [x5x], xll=True)

# ---- X6: file round trip: a by-request volatile cell saved clean; open and calculate ----
x6 = standard(["=VTrue()", "=VPlain()", "=RAND()", "=SUMIF(C1:C3,\">1\",D1:D2)", "=Bang()", "=VFalse()+Bang()", "=Hu(C1)"], id="x6", clean=2, post_steps=[
    {"op": "reopen", "file": "final", "tag": "open1"}, C("o1"), C("o2"), {"op": "save", "tag": "s_reopened"}, {"op": "reopen", "file": "s_reopened", "tag": "open2"}, C("p1")])
# the xll is needed to evaluate Bang and Hu after reopen; formulas using it are in the same workbook
spec("x6", [x6], xll=True)

# ---- X7: dependents ----
x7_cells = [("F1", "=A1&\"\""), ("F2", "=F1&\"\""), ("F3", "=Cnt(A1)"), ("F4", "=Cnt(A1&\"\")"), ("F5", "=A2*0"), ("F6", "=Cnt(F5)"), ("F7", "=Cnt(F5&\"\")"),
            ("F8", "=SUM(A1:A2)"), ("F9", "=A3+0"), ("F10", "=Cnt(A3&\"\")"), ("F11", "=Cnt(A4&\"\")")]
x7 = standard(["=VSeq()", "=RAND()", "=C1+0", "=INDIRECT(\"C1\")"], id="x7", warm=2, clean=2, vcfg=[("VSeq", "T")], extra_cells=x7_cells, final_save=False,
              post_steps=[{"op": "save", "tag": "s_vol"}, {"op": "vcfg", "name": "VSeq", "script": "F"}, C("f0"), C("f1"), C("f2"), {"op": "save", "tag": "s_cleared"}])
spec("x7", [x7])

# ---- P1f: SUMIF decided per evaluation from the shapes it actually gets ----
P1F = ['=SUMIF(C1:C3,">1",D1:INDEX(D:D,G1))', '=IF(TRUE,1,SUMIF(C1:C3,">1",D1:D2))', '=IF(FALSE,1,SUMIF(C1:C3,">1",D1:D2))', '=SUM(SUMIF(C1:C3,">1",D1:D2),1)',
       '=SUMIF(C1:C3,">1",D1:INDEX(D:D,G1))*0', '=AVERAGEIF(C1:C3,">1",D1:INDEX(D:D,G1))']
steps = [C("w1"), C("w2"), C("a1"), C("a2"), {"op": "save", "tag": "s_g2"},
         {"op": "set", "addr": "G1", "value": 3}, C("b0"), C("b1"), C("b2"), {"op": "save", "tag": "s_g3"},
         {"op": "set", "addr": "G1", "value": 2}, C("c0"), C("c1"), C("c2"), {"op": "save", "tag": "s_g2again"}]
p1f = standard(P1F, id="p1f", extra_consts=[("G1", 2)], final_save=False, warm=0, clean=0, post_steps=steps)
spec("p1f", [p1f])

# ---- P6: run-time decided volatility (name use, SUMIF shape, spill block) against user requests, both orders ----
P6 = ["=NmRand+VFalse()", "=VFalse()+NmRand", "=NmLam()+VFalse()", "=VFalse()+NmLam()", "=NmInd+VFalse()", "=VFalse()+NmInd",
      '=SUMIF(C1:C3,">1",D1:D2)+VFalse()', '=VFalse()+SUMIF(C1:C3,">1",D1:D2)', '=SUMIF(C1:C3,">1",D1:D2)+VTrue()', '=VTrue()+SUMIF(C1:C3,">1",D1:D2)',
      '=SUMIF(C1:C3,">1",D1:D2)+Hu(C1)', '=NmVba+SUMIF(C1:C3,">1",D1:D2)', '=VFalse()+IF(TRUE,SUMIF(C1:C3,">1",D1:D2),0)',
      "=NmRand+IF(TRUE,1,RAND())", "=VFalse()+NmRand+VFalse()", "=NmVF+NmRand", "=NmRand+NmVF"]
spec("p6", [standard(P6, id="p6", trials=3, names=NAMES)], xll=True)

if __name__ == "__main__":
    os.makedirs(os.path.join(HERE, "specs"), exist_ok=True)
    wanted = sys.argv[1:] or list(SPECS)
    for name in wanted:
        with open(os.path.join(HERE, "specs", name + ".json"), "w") as f:
            json.dump(SPECS[name], f, indent=1)
    print("specs written:", " ".join(wanted))
