# Two-tail Student-t inverse stages (dc-bc1o), 2026-11-12

Codex, dna-koderbot. Existing DNA Calc code and public Excel observations only.
No Microsoft source or disassembly.

## Predictions before calibration

The captured T.INV.2T(2/15,6) is one ulp low. Its public beta-inverse composition
was already measured, but the current inverse-beta lattice point can be wrong.
First isolate its final arithmetic without changing that point: compare ordinary
and stored-PC64 reciprocal, subtraction, degree multiplication and square root
(16 masks). Reuse the existing regularized-beta kernel and lattice search,
then require the ordinary result to reproduce calc_func::invoke. If no mask
fixes the captured result, do not attribute that row to these publication sites
or infer new inverse coefficients. This is retained-case calibration; no fresh
Excel answers are claimed.

All 16 masks reproduce the same captured low result ...f1cc; the chosen beta
coordinate is 0x3fe54e0a27148f53. The ordinary model reproduces invoke exactly.
Thus no final-operation precision change fixes this row. Before new queries,
freeze current GAMMALN(0.5/3/3.5), BETA.INV(2/15,3,0.5) and BETA.DIST at
17 adjacent beta coordinates. Measure whether the input normalization/forward
CDF differs, or whether Excel's inverse selects a different point on an otherwise
matching forward surface. Do not infer a solver rule from one plateau.

All three GAMMALN leaves and all 17 adjacent forward BETA.DIST values match
Excel bit for bit, but Excel BETA.INV chooses ...8f52, one below the current
...8f53. The lower point's published CDF is two probability ulp below p and
the upper point is one above p, so choosing the nearest published CDF does not
explain this inverse. The correctly rounded mathematical beta root, evaluated
from its exact integer/half-shape polynomial at 100 digits, is ...8f52; its
mathematical T quantile also equals the captured ...f1cd.

Before new targets, test accuracy versus endpoint selection on 256 new calls
with integer a=1..16, b=1/2, p in (.01,.99), plus the captured calibration case.
Freeze the current invoke, its lower neighbour, nearest-forward and mathematical
roots. The mathematical roots come from the exact finite polynomial for
I_z(a,1/2), with independent 80/100-digit agreement. Also freeze T.INV.2T
predictions from the rounded mathematical beta coordinate and from the exact
mathematical T root. A poor fit rejects accuracy alone; do not fit coefficients
or bias the publication to one observed point.

The broad half-shape holdout rejects a universal lower-endpoint rule: beta
current 158/256, lower neighbour 85/256, nearest-forward 192/256, mathematical
190/256. T inverse current 184/256, mathematical T root 37/256, composition
from rounded mathematical beta 195/256. Thus greater mathematical accuracy is
not a complete Excel program, and no new root publication rule lands.

A separate interpretable special is df=2: BETA.INV(p,1,1/2) has inverse
z=1-(1-p)^2=p*(2-p). Before new queries, replay its 27 retained cases with
naive/stable forms, raw power/exp-log and expm1/log1p forms, then compose the
published T formula. This is retained-case calibration only. A strong special
shape needs new frozen probabilities and shape controls before implementation;
it would not resolve the df=6 captured root.

The df=2 closed-form calibration is weak (best beta 17/27, T 18/27), so no
unit-shape shortcut lands. The broader beta nearest-forward result instead
shows a measurable improvement (60 gained, 26 lost of 256; net +34), although
it does not explain the captured exception. Before new queries, qualify that
publication rule on 384 general/scaled beta inverses, with endpoint controls.
Freeze both invoke versions first. Reuse the existing T.INV.2T nearest-lattice
rule through the shared beta-inverse kernel rather than duplicating it; T inverse
results must remain identical. Inspect BETAINV's existing 62-row bank before
paying for a full batch. Keep all losses explicit and reject a corpus regression.

The 384-call general/scaled invoke holdout gives nearest publication 154 ->170:
35 gained and 19 lost (unit 84 ->103/256; scaled 70 ->67/128). The existing
BETAINV bank gives 25 ->28/62, four gained and one lost (oc56). This is a net
gain with known exceptions, not complete identification; a corpus regression
would reject it. Endpoint controls exposed an independent issue: the current
lattice search starts with a strict lower bracket, which need not hold at p=0.

Before new queries, predict direct support bounds for BETA.INV at p=0/1 after
shape/bound validation. Contrast direct bounds with the affine 0/1 calculation
and with rejecting an overflowing width. Include severe finite bounds and
invalid-shape controls to measure validation order. Predict T.INV.2T(1,df)=0
after valid df, contrasting probability-first and domain-first guards. These
are mathematical endpoint hypotheses; do not infer an answer from the existing
not-modelled result or remove a guard without evidence.

The mathematical beta endpoint hypothesis is rejected: all 42 valid p=0/1
cases, including severe finite bounds, return #NUM!; all four invalid-shape
controls also return #NUM!. The separate earlier holdout's 48 endpoint calls
agree. Excel beta inverse therefore has an open probability interval; do not
return the support bounds just because they are mathematical limits.
T.INV.2T instead matches domain-first zero at p=1 on all 11 controls, including
df=1,2,3,6,6.6,100,1e10 and invalid degree limits. Before implementation, freeze
those same expectations for BETAINV/TINV aliases and verify them directly.

BETAINV and TINV aliases exactly reproduce all 46 and 11 endpoint answers.
The candidate now rejects beta p=0/1 and returns +0 for two-tail t p=1 after
degree validation. Native and software-x87 invoke tests pass the endpoint rules
and four gained BETAINV controls; fresh Excel 20527 rechecks agree with those
four older bank answers. The lost bank oc56 remains a real one-ulp loss on
Excel 20527 as well and will be listed explicitly if the net-gain batch lands.
No candidate fixes the original df=6 inverse root; that gap stays open.

## Landing evidence

Final invoke replay of all 384 frozen holdout inputs, now including the verified
endpoint guards: 154 ->218 matched, 83 gained and 19 lost. Lost case IDs:
15, 51, 93, 119, 188, 204, 206, 268, 276, 303, 314, 317, 325, 356,
361, 364, 370, 373, 383. Their complete arguments and bit patterns are in
`beta-inverse-final-comparison.json`; all losses are counted.

Full bank: 3,895,333 ->3,895,336 /3,910,795 matched, 15,459 failed
(4 unread, 2,064 excluded). BETAINV 25 ->28/62, four gained and one lost;
12 actuals changed. All other functions' actuals are unchanged. Streaming
comparison verifies all 3,910,791 decoded input/selection/answer/source/environment
identities. The single lost row is bank20131/BETAINV.jsonl oc56:
args 0x3f947ae147ae147b,0x4018000000000000,0x4000000000000000;
Excel/old 0x3fd9e16ca7f1b09f, new 0x3fd9e16ca7f1b09e. A fresh Excel
20527 query confirms the old reading, so this is a real one-ulp loss.

Oracle corpus unchanged at 208,197 /212,841, 4,644 failures; zero gains or
losses across all 212,217 recorded cells, with the same inventory/cases.
The initial check rejected the 12 changed pinned computed results. The
repository pin tool updates those results; all 62 inputs, Excel answers, source
records and selection are verified unchanged. The full-bank status is retained
from the final full judge, not the narrow pin. The unchanged witness assertion
then passes. Final check: 732 passed, 0 failed, 3 ignored and 1 not run across
13 harnesses, workspace/wasm checks passed, 19.385 seconds. Native/software-x87
focused endpoint and gained-control tests also pass.

Evidence: dnacalc-evidence/research/round57/20261112-fn2-inverse-0810/,
103 files, 154,664,921 bytes; checksum copy and one-way check zero differences.
No F inverse, gamma inverse or longer beta recurrence alternative lands.
