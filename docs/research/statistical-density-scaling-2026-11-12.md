# Statistical density scaling (dc-bc1o), 2026-11-12

Codex, dna-koderbot. Sources: existing DNA Calc code, captured input workbooks
and public Excel answers. No Microsoft source or disassembly.

## Beta scaling hypothesis before queries

The captured BETA.DIST density is BETA.DIST(2,2/15,2/15,FALSE,1,6):
expected 0x3fb11fa08daea546, current 0x3fb11fa08daea548. The unit-interval
kernel is already measured, but the non-unit route still uses a logarithmic
formula with Lanczos normalisation. Hypothesis: Excel evaluates its unit density
at z=(x-lower)/(upper-lower), then divides by the interval width. Rival shapes
multiply by the reciprocal width, normalise from the upper endpoint, or use
stored-PC64 arithmetic at normalisation/scaling sites. No constants are fitted.

Freeze deterministic cases and all candidate normalised coordinates before
asking unit BETA.DIST. Then freeze every numerical composition from those
public primitive answers before asking the scaled target calls. Include the
captured inputs and independent small/fractional/large shapes, dyadic and
non-dyadic widths and translated intervals. A composition alone is not a
product fix: after identification, verify the route through calc_func::invoke
and require a full-bank/corpus gain before landing.

## First result and precision predictions

The lower-endpoint coordinate followed by division matches 192/192 fresh scaled
answers, including the captured case. Reciprocal multiplication matches 159/192,
upper-endpoint normalisation 140/192. Binary64 versus stored-PC64 choices are
indistinguishable on this batch. Raw: beta-scaling-{inputs,primitives,predictions,answers}.json.

Before the next query, predict the same graph on an independent 256-case holdout.
Mine fresh cases where ordinary and PC64 division produce different final bits,
using z=1/2 to isolate scaling from coordinate rounding. Separately mine x/width
coordinate disagreements and ask unit densities at both candidate coordinates.
Freeze all composed predictions before the scaled calls. No thresholds or
coefficients are tuned to answers; the cases are selected by model disagreement.

The final-division disagreement batch selects PC64 on 32/32 (ordinary 0/32).
The coordinate batch selects PC64 on 32/32 (ordinary 6/32). The independent
256-case continuous-shape holdout matches 256/256 with PC64 normalisation and
scaling. Production now uses that graph where the existing measured unit-density
kernel is available. Its unsupported-shape fallback and the CDF retain their
existing route. Four invoke controls pass, including the captured case and
separate coordinate/final-rounding controls. Full batch validation is pending.

## F density, predictions before queries

The captured F.DIST(2,6.6,8,FALSE) truncates to degrees 6 and 8 and is 11 ulp
off. Unlike the measured beta density, the current F density uses Lanczos gamma
values in a direct formula. A direct exact-integer-gamma composition predicts
...40a7, whereas the captured target is ...40a3, so improving gamma alone cannot
fix that case. Before a new query compare public beta-density compositions:
p=m*x/(n+m*x), q=n/(n+m*x); unshifted beta(a=m/2,b=n/2) times the Jacobian,
and beta(a,b+1) times m*q/(m+n), each in both orientations and stored/plain
arithmetic. Freeze coordinates before primitive queries and compositions before
F.DIST target queries. This identifies or rejects the rounded public composition;
a failed composition does not justify fitting constants.

The rounded public beta compositions are rejected: best 13/192, and none fixes
the captured F row. Before the next queries, use the density's mathematical
binomial identity instead: for m>=2, k=m/2-1, N=(m+n)/2-1, density equals
BinomialMass(k,N,p,q)*(m*q/2). This uses the existing measured mass kernel
before beta's extra normalizer term, so a public beta density need not reproduce
it. For even degrees k and N are integers and can be measured directly with
BINOM.DIST, restricting this identification batch to coordinates whose q equals
the public call's 1-p. Compare multiply versus carrying the scale into the
exponent only if the stored-mass composition fails; fit no constants.

The stored public binomial-mass composition is rejected (11/192). A product
candidate carrying the scale into the existing running exponential's normalizer
replayed only 14/192 earlier F answers and still missed the captured row. This
was a calibration replay, not a fresh holdout; it was removed before new queries.
No F change lands from these failed shapes.

The existing witness-read predictor was used through calc_func::invoke for the
512 beta cases. On Linux it durably writes model predictions and then refuses
native Excel capture; the actual answers remain the independently retained
service readings. Product replay is 408/512, not the perfect primitive composition
scores: existing unit-density errors and unsupported shapes are still present.
Four focused product regression controls pass. Do not claim 512 product passes.

The read-only beta review found no code defect and verified all earlier echoed
inputs, freeze times and scores. It noted that the precision batches isolate
divisions with lower=0, not endpoint subtraction separately. Before querying
again, construct 32 width-subtraction and 32 numerator-subtraction disagreements
using exact dyadic intervals and tiny negative lower endpoints. Uniform and
linear beta densities isolate the relevant stage. Freeze every coordinate,
primitive composition and whole-function prediction before the target calls.

The numerator-subtraction control selects PC64 on 32/32, ordinary 0/32.
The proposed width controls all return #NUM! at original x=0, so they identify
no numeric width rule and are not counted as matches. Their unit primitive
calls are numeric; retain this unexpected whole-function edge separately.
Before retrying width, move x to the interval midpoint (uniform density, still
1/width) with the same frozen bounds, and freeze both width predictions anew.
Also ask ordinary translated x=0 controls to distinguish that endpoint issue.

The midpoint width retry selects PC64 on 32/32 versus ordinary 3/32, independently
of the already measured numerator subtraction (32/32 versus ordinary 0/32).
The arithmetic stages in the selected graph are now separately distinguished.
Two resulting controls were added to the invoke regression alongside the four
earlier controls. The x=0 check is a separate pre-existing edge: BETA.DIST at
x=0 with alpha=1 returns #NUM! even on translated intervals, while alpha=2
controls return their finite densities. The scaling change does not claim to
model that edge. Raw: beta-subtraction-*, beta-width-* and beta-zero-controls-*.

## Final beta batch validation

Unit-route qualification matches 408/512 unit answers; the scaled route also
matches 408/512: 406 both correct, 102 both incorrect, two unit errors rounded
away by scaling and two retained unsupported-shape fallbacks missing despite
correct unit results. No claim is made that the full unit kernel is exact.
The first unit predictor request rejected duplicate calls; its explicit
--repeat-calls retry supplies the qualified results. The analyzer's initial
wrong-file selection was corrected before publication (unit-product-qualification.json).

Full bank: 3895333/3910795 matched, 15462 failed (4 unread, 2064 excluded),
unchanged. The trace comparison decodes 3910791 identical-input rows with +0/-0
and zero actuals changed. The initial --report run was read-only, so the judge
was repeated without that option: identical counts, with witness-status.tsv
written by this final full-bank run. No manually generated status is committed.

Oracle corpus: 208191 -> 208192 / 212841 passes; failures 4650 -> 4649
(3138 mismatched, 771 refused, 545 failed/missing; all harness qualifiers retained).
All 212217 recorded cell rows compared, including sectioned oracles: +1 BETA.DIST,
-0 and no other verdict transitions. The gained actual bits equal the baseline
live Excel expected bits at Sheet1!B4 in workbook 16a2f714110e... . The 516 timeout
and 108 awaiting-capture cells remain failures. Lost rows: none.

cargo xtask check passes: 728 tests passed, none failed, three ignored and one
not run in 13 harnesses, plus the touched portable package's wasm check; 117.816 s.
The read-only review found no code defect and verified the initial precision
holdouts; the follow-up subtraction probes resolve its stated evidence limit.

Raw evidence: dnacalc-evidence/research/round54/20261112-fn2-beta-0445/,
104 files, 149841629 bytes; checksum copy and one-way check: zero differences.
The preceding SUM/CORREL batch is 384fb91d. No F, hypergeometric or Student t
change is included; their rejected/ongoing experiments are retained in this batch's
research logs. dc-bc1o remains in progress for the other statistical roots.
