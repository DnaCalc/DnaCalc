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

## Shape-two beta inverse, before retained-case calibration

The exact defining integral for BETA(a,2) is x^a*(1+a*(1-x)), with
derivative a*(a+1)*x^(a-1)*(1-x). Test whether its mathematical inverse
explains the remaining shape-two bank rows, including the real oc56 loss,
before considering any solver change. Use Decimal 80/100-digit roots as
independent calibration, not product behavior or an Excel pass. A special
requires a strong fresh frozen shape-specific holdout and a stable general
computation; do not select a row-specific adjustment or fit constants.

The exact shape-two mathematical inverse is rejected on the retained bank:
3/8 match versus current 4/8. It recovers oc56 but loses other existing
matches. Both Decimal precisions agree. No special case or fresh target
query follows from this weak result.

Further rejected-model evidence: dnacalc-evidence/research/round58/20261112-fn2-rejected-0830/,
64 files, 120,226,186 bytes; checksum copy and one-way check zero differences.
These records do not add a landed pass or change any Excel answer.

## Expanded-bank T-family computation controls (fix2)

Fresh Excel 20527 controls, retained under CalcFunc-Evidence
`probes/2026-11-12-fix2/`, separate wrapper admission from the inverse kernel:

- T.INV is the signed T.INV.2T call at `2*p` below the median and
  `2*(1-p)` above it: 524/524 paired controls, including probabilities down
  to 1e-100. Reflecting the input probability first loses the small lower tail.
- T.INV.2T and TINV admit `1 < p < 2` and return a negative quantile.
  Reflect to `2-p` before forming the Cauchy angle as well; 127/127 fresh
  p>1 Cauchy controls match the reflected call. Direct cotangent staging
  on p>1 matched 9/127. On p<1 the public cotangent matched 131/131.
  The df=1 pole at p=0 is #DIV/0!; other degrees reject it with #NUM!.
- For df>=2, the public inverse uses `sqrt(v*(1/z-1))`, with
  `z=BETA.INV(p,v/2,0.5)`, through p=0.9. Above 0.9 it uses
  `sqrt(v*y/(1-y))`, with `y=BETA.INV(1-p,0.5,v/2)`.
  This public composition explains 278/280 fresh controls. The two unexplained
  controls both have df=1e10; they remain counted. A single unit of the beta
  coordinate can be many units of the Student-t result.
- CONFIDENCE.T first stores `s/sqrt(n)`, then multiplies the quantile:
  512/512 fresh scale controls versus 330/512 for multiply-then-divide.
  A truncated sample size of one returns #DIV/0! after the other domains.
- T.DIST.RT accepts negative x; TDIST and T.DIST.2T reject it. T.DIST's
  density admits degrees above the CDF ceiling; zero truncated degrees
  return #DIV/0! in density and #NUM! in the CDF. Squaring overflow is #NUM!.
- TDIST's tails conversion shares the existing date/count integer floor.
  The three adjacent-input transitions at 1, 2 and 3 are exactly
  `2^-22 + 2^-33` below the integer, inclusive.

The half/half beta integral follows the public arcsine graph. The first
257-row set matches 243/257 with direct `2*ASIN(SQRT(x))/PI()`; every miss
is above 0.9. A fresh 518-row set matches 517/518 after a stored-PC64
quotient and the complementary coordinate above 0.9; the remaining row
at exactly 0.9 selects the complementary arm. The immediate neighbours
of 0.9 and a near-one discriminator are retained. On 512 fresh Student-t
df=1 inputs, its two-tail answer equals the public beta call at stored
`1/(1+t*t)` on every row. This arm forms its own `1-x`; retaining the
wrapper's accurate second argument does not reproduce Excel near zero.

The beta inverse remains a separate gap. Excel's extreme inverse sometimes
fails to invert its own CDF: BETA.INV(1e-100,500,0.5) gives
0.22605101582053366, where BETA.DIST is zero, while the calculated root
0.6349801585984094 has worksheet CDF 1.0000000000000076e-100.
These are repeatable observations, not permission to fit an input-specific
answer. A proposed unit-shape analytic inverse was rejected: on 512 fresh
controls, EXP(LN(p)/a) explains 403 results and POWER explains 269;
the complementary formula explains only 138/512. No such shortcut lands.
The frozen full-bank population (Evidence 50f76220, same readings as de4e050e)
is 4,663,392 rows. A fresh full before/after comparison gives 2,546 gained
matches, 19 lost, net +2,527; 2,967 actuals changed, with every recorded input,
selection, answer, source and environment unchanged across 4,663,388 decoded
rows. The result is 4,541,006 matched and 122,386 failed, including four unread;
2,064 excluded. All four forward functions gain with no losses: T.DIST
3659->4009/4281, T.DIST.RT 2989->3819/4184, T.DIST.2T
3665->3886/4178, TDIST 4233->4260/4363. Their worst remaining numeric gaps
are respectively 9091, 6583, 5954 and 1687 ULP. The inverse/confidence beads
stay open. All 19 losses (one to seven ULP) are listed in the source commit;
BETAINV oc11 is the explicitly classified one-ULP pinned loss against dc-b0p4.
Thirteen calculated predictions change, with inputs and Excel answers preserved.

The corpus is unchanged at the same source baseline: 77039 pass, 1225 fail
of 78264 cells; all 78156 recorded cell verdicts unchanged, with the same
108 awaiting capture. The intervening financial work accounts for the increase
since the previous fix2 batch. Native and software-x87 f1_stats each pass 82
cases. The final light check passes 746 tests, zero failures, with workspace
all-target and wasm checks passed. The full generated status survives pin refresh
unchanged. Evidence controls are in 50f76220 and 211faa3b; the latter also
retains rejected inverse and follow-up continued-fraction discriminators.
R2: dnacalc-corpus/runs/dna-koderbot/20261112-fix2-batch3/ (8 files, 303418250 bytes; checksum copy and one-way check passed).
