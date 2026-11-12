# SUM cancellation (dc-ss3b), 2026-11-12

Implementer: Codex on dna-koderbot. Sources: DNA Calc code and Excel outputs
(tier 1); no Microsoft source, disassembly or restricted code read.

## Predictions before queries (03:31 UTC)

The expression evaluator already adjusts a final infix + or - when its left
operand's binary exponent exceeds the result's by at least 50. Parentheses,
function calls and an additional final operation suppress that adjustment.
SUM itself currently left-folds binary64 values and has no cancellation rule.

Hypothesis A: SUM applies the same 50-exponent cancellation after each addition.
Hypothesis B: SUM adjusts only its last nonzero addition; a trailing zero may
suppress adjustment. Hypothesis C: SUM does not snap, and saved zeros in the
bead are stale or serialisation differences. Hypothesis D: compensated summation.

Predictions discriminating these shapes:
SUM(0.1,0.2,-0.3): A/B -> 0; C -> 2^-54; D -> 2^-55.
SUM(0.1,0.2,-0.3,1e-20): A -> 1e-20; B/C retain the ~2^-54 residual.
SUM(1,2^-50,-1): exponent difference 50 -> 0 under A/B; 2^-50 under C/D.
SUM(1,2^-49,-1): exponent difference 49 -> 2^-49 under A/B/C/D.
SUM(1,1e-16,-1,1e-16): A/B/C -> 1e-16; D -> 2e-16.
SUM(1e16,1,-1e16): A/B/C -> 0; D -> 1.
SUM(1.333,1.225,-1.333,-1.225): A/B -> 0; C -> binary64 residual.
Reversals, grouping as one precomputed argument, zero tails, and powers-of-two
scaling discriminate the accumulation sites and relative versus absolute bands.
The service permits typed function calls, not expression text: expression and
range cases require the session job route; do not invent expression answers.

## First answer (03:31:09 UTC)

Excel 20527, env_id 29bedb2b6722, v1 file queue accepts numeric arrays as well
as scalar calls (its deployed policy is wider than this checkout's policy).
SUM(0.1,0.2,-0.3) = +0, but a trailing numeric zero preserves 2^-54.
A 1e-20 tail preserves the accumulated residual. The two power-of-two controls
separate exactly at exponent gap 50. These observations select B: one adjustment
at the final addition, even when that operand is zero. They reject A, C and D.
Raw: ~/codex-runs/fn2/sum-probe-1.json (eight calls, unchanged echoed inputs).

## Next predictions, frozen before queries (03:32 UTC)

A numeric array should behave as its flattened numeric sequence. An array's
trailing text/blank/logical should be skipped and should not be an addition.
Numeric zero is an addition. Each array/reference may instead calculate its own
subtotal, so separating one cancelling array and a zero scalar discriminates
per-container snapping from one global final adjustment. Predict:
SUM({0.1,0.2,-0.3}) = 0; SUM({0.1,0.2,-0.3},0) = 2^-54 if global,
0 if arrays snap separately. SUM(0,{0.1,0.2,-0.3}) = 0.
A precomputed 0.1+0.2 supplied as one numeric argument and -0.3 as the last
should snap; a zero tail should preserve its residual. Test both operand
orders around the binade boundary at 2 and residuals k*2^-52: the left
partial sum should set the threshold, not the maximum magnitude in the list.
For each power-of-two scale, k=1..7 around 1 should snap and k>=8 should stand.

## Second answer (03:32 UTC)

All 98 calls follow the global final-addition rule, including array boundaries,
trailing zero, positive and negative scales 2^-900..2^900, and the asymmetric
binade cases. In particular k=4 at 2 snaps with positive left operand, and
stands after reversal. Raw: ~/codex-runs/fn2/sum-probe-2.json.

Before the third query: predict that array-held text/logicals/blank tails are
skipped, preserving the final numeric addition and its snap; a numeric zero tail
is retained and suppresses that snap. Direct cell-reference text/logicals/blank
are skipped too. Predict AVERAGE, SUMSQ and SUMPRODUCT do not acquire this rule
merely because SUM has it: measure each independently before changing a sibling.

## Sibling control and next predictions (03:34 UTC)

Contrary to the initial sibling hypothesis, AVERAGE({0.1,0.2,-0.3}) is zero.
SUMPRODUCT of that array preserves 2^-54. The mixed-array query was refused
(`'bits'`); it supplied no answers and is not used as evidence.
Predict AVERAGE/AVERAGEA adjust the numerator once before division: a numeric
zero tail should preserve 2^-54 / 4, not zero. At the exponent-gap boundary,
mean([1+8*2^-52,-1]) should be 4*2^-52 (numerator retained); if the rule were
after division against the left partial sum it would wrongly become zero.
Predict SUM over trailing cell text/logical/blank skips those values, unlike
zero. Keep the mixed-array refusal separate from these scalar-cell questions.

## Final controls and implementation

The fourth query confirms that trailing cell text, logical and blank are skipped,
while numeric zero suppresses the earlier snap. Both AVERAGE and AVERAGEA
adjust their sum before dividing: the exponent-gap-49 numerator survives division
even when that division lowers its exponent. SUMPRODUCT retains 2^-54.
Raw: ~/codex-runs/fn2/sum-probe-4.json. The production change retains the
last partial sum before a numeric addition and applies the shared final-sum
adjustment once. CalcFml calls that same helper; its existing root-placement rule
and rounding sites are unchanged. Native invoke tests cover direct, array and
reference inputs, zero and skipped tails, signs and exponent scales.

## Batch validation

Final full-bank judge: 3,895,333/3,910,795 matched, 15,462 failed (4 unread,
2,064 excluded); 295 functions, 228 exact on every counted row. Existing SUM
1,211/1,211, AVERAGE 1,222/1,222 and AVERAGEA 1,212/1,212 remain exact.
The bank comparison decodes 3,910,791 rows with unchanged inputs, selection,
answers, sources and environments: +0/-0 matched rows and zero actuals changed.
Thus the bank is neutral; the new cancellation behaviour is evidenced by fresh
Excel probes and invoke regressions. The 1,297 saved-cache Enron cells were not
individually recaptured and are not claimed as new live-Excel passes.

`cargo xtask check` passes: workspace check, 909 tests passed, zero failed,
three ignored and one not run in 16 harnesses; 125.820 seconds. Raw ordered
output: ~/codex-runs/fn2/check-final-ordered.log.

Final corpus: 351 workbooks, 350 compared and one timeout; 208191/212841
passes (97.81%), 4650 failures: 3139 mismatched, 771 refused and 545 failed or
missing, with external/circular/awaiting qualifiers retained by the harness.
Baseline was 207753 passes and 5088 failures on the same inventory. Comparing
all 212217 recorded cell rows, including sectioned oracles, gives 438 gains
(all CORREL), zero losses and no other verdict transitions. Every gained actual
bit equals its baseline live Excel expected bit. The 516 timeout cells and 108
awaiting-capture cells remain failures. Lost rows: none.

Large raw evidence: `dnacalc-evidence/research/round53/20261112-fn2-0327/`,
97 files, 451038645 bytes. Copy used --checksum; one-way checks reported zero
differences (the final 96-file local stage plus the earlier verified intermediate
bank trace retained only in R2). Baseline, intermediate and final corpus archives,
compressed bank traces, predictions, echoed answers, comparisons and ordered
check output are retained there. No raw corpus output is committed.
