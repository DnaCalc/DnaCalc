# CORREL arithmetic (dc-wli1), 2026-11-12

Implementer: Codex on dna-koderbot. Sources: DNA Calc code, existing captures,
and Excel answers (tier 1). No Microsoft source, disassembly or restricted code.

An initial service capability check replayed the existing five-point affine
unit-test input and confirmed 0x3feffffffffffffe on build 20527. That query was
not a new numerical prediction: its expected value already appears in
paired_stats_common.rs. All new probes below freeze candidate predictions first.

## Shape hypotheses before the first identification batch (03:35 UTC)

The current route takes two passes: left-fold means, then centred sums of
products and squares, divides each by n-1, and divides covariance by the
product of two binary64 square roots. Candidates vary one part at a time:

- mean: sequential binary64 sum; compensated sum; first-value shifted sum;
  incremental online update;
- centred sums: sequential forward/reverse; compensated; shifted/online;
- normalisation: no divisor, n, n-1;
- final denominator: sqrt(x)*sqrt(y), sqrt(x*y), or sequential division by
  the two roots. Root storage may be PC64 x87 rounded then stored rather than
  a single binary64 sqrt; only investigate this after selecting the shape.

Prediction: removing the n-1 divisions should explain a large fraction of
last-bit differences without changing the mathematical correlation; covariance
and standard deviations should not need sample normalisation to cancel out.
Five-point affine controls cannot distinguish that from the current shape.
Asymmetric short data, large offsets and cancellation-heavy means can.
Freeze deterministic input seed and every candidate result in a JSON file
before submitting those inputs to Excel; report full match counts and holdouts.

The first batch was unqualified because Excel changed an input (a decimal
case included negative zero). It supplied no usable answers. Before retrying,
canonicalise zero inputs to positive zero, freeze all 216 inputs and 108
candidate predictions in correl-probe-2-predictions.json, then ask CORREL,
PEARSON and RSQ on that unchanged batch. No fitted constants are involved.

## First identification result (03:37 UTC)

216 deterministic cases, n=3..32, six input families (uniform, large offset,
decimal, mixed scale, affine, nearly affine). Sequential binary64 means and
centred sums, population normalisation n, and separate square roots match
216/216 CORREL answers. Current sample normalisation matches 102/216; no
normalisation matches 119/216. Compensated means with otherwise selected shape
match 159/216; compensated centred sums match 119/216. No constants fitted.
Frozen predictions and echoed-input answers: ~/codex-runs/fn2/correl-probe-2-*.

Before holdout: the selected shape should also match PEARSON, and RSQ should
be its stored binary64 result squared. Predict the same on independent seed
0xDC01112027, n=2,11,13,19,23,29,33,47,64,127,128, at large/small scale,
with forward/reversed vectors and affine controls. Freeze all predictions
before querying this independent batch. Existing captures supply additional
held-out long vectors (59, 63, 223 cells), measured through the product route.

The PEARSON prediction matches 216/216. The population-based RSQ prediction
is falsified: 102/216. The original sample-based correlation, squared, instead
matches all 216/216 RSQ answers. Preserve that independent observed route by
sharing the centred-sum implementation with an explicit divisor choice.
Before the holdout request, predict population CORREL/PEARSON and sample RSQ.

## Holdout result

Independent 198-case batch (n=2..128): selected population shape matches
198/198 CORREL and 198/198 PEARSON; retained sample shape squared matches
198/198 RSQ. The combined 414 inputs discriminate these functions' normalisation
without any fitted coefficient. Square-root precision changes were unnecessary.
Raw: ~/codex-runs/fn2/correl-holdout-{predictions,answers}.json.

## Corpus holdout: four residual cells

The first product corpus after the population change gains 434 of the original
438 CORREL mismatches, with no lost passes in completed workbooks. Four cells
(two distinct input pairs, duplicated in two books) still differ by one ulp:
U18 (220 pairs, actual ...187d, Excel ...187c) and T21 (217 pairs, actual
...04a7, Excel ...04a6). Do not close dc-wli1 while those remain.

Before further queries, predict that a rare stored-PC64 operation explains
these residuals: compare single-round and stored-PC64 means, centred arithmetic,
accumulation, normalisation, roots and final operations independently. The shared
numeric core already supplies these primitives. Freeze predictions before asking
SQRT on the computed variances and before replaying the full pairs to Excel.

Fresh Excel replays confirm both residual target bits. U18 has 127 numeric
pairs after blanks are skipped; T21 has 217. SQRT of all four plain variances
agrees with single rounding, rejecting the root hypothesis on these cases.
Of 64 independent precision masks, precisely those with stored-PC64 running
additions match both answers. Other stage choices do not distinguish these two.
Raw frozen masks, SQRT answers and fresh CORREL answers: correl-residual-*.json.

Before the next query: retain plain means, differences/products, normalisation,
roots and final scaling, and use stored-PC64 additions for the three running
centred sums. Replay the earlier 414 answers locally, then generate fresh inputs
using seed 0xDCAC2026, select inputs where this differs from plain summation,
freeze both predictions, and only then ask Excel. Do not tune any constants.

The accumulation candidate preserves all earlier 414 matches. Mining 40,000
fresh inputs selected 92 disagreements before Excel; plain accumulation matches
0/92 and stored-PC64 accumulation matches 91/92. One remaining fresh input
therefore distinguishes another stage; investigate it before changing the kernel.
Raw: correl-accumulation-{predictions,answers}.json.

The remaining mined input requires stored-PC64 first-pass arithmetic as well.
Masks using PC64 means and running additions all match 92/92, and preserve
414/414 earlier answers; those two stages are now independently distinguished.
Before the next fresh batch: compare the remaining 16 combinations of centred
arithmetic, normalisation, roots and final scaling, with the identified mean and
accumulator stages fixed to PC64. Select only disagreements by prediction,
freeze all mask answers, then query CORREL/PEARSON. For RSQ, freeze the same
sample-normalised masks and both ordinary and PC64 final squares before asking.

## Second precision holdout

A new 40,000-input search selected 47 short vectors where the remaining masks
disagree, all predictions frozen before requests. CORREL and PEARSON each
match 47/47 only with PC64 centred arithmetic, roots and final scaling as well
as the already selected means/accumulators. Ordinary versus PC64 division by
these small integer counts remains indistinguishable; the consistent stored-PC64
arithmetic graph fits every result. Sample normalisation with that graph also
matches all 47 RSQ answers; both square publication choices agree on this batch,
so preserve RSQ's existing binary64 square.
Raw: correl-precision-holdout-{predictions,answers}.json.

The revised production route uses the existing stored-PC64 numeric primitives
throughout the two-pass correlation, retaining the sample divisor for RSQ.
The other paired statistics retain their existing arithmetic via the same
parameterised mean helper. This is an exactness change, not a speed change.

A separate three-point first-pass control, frozen before its query, distinguishes
the means directly: x=[1.5, 2^-53+2^-66, -1.5], y=[1,2,1]. Stored-PC64
means predict 0x3c8279a74590331c; ordinary means predict zero. Excel returns
the former. Raw: correl-mean-control-{predictions,answers}.json.

The first corpus run (population change only) finished at 208187/212841 passes,
434 gained and no lost passes versus baseline. Its four residual cells prompted
the precision work above. Final full-bank and corpus validation must use the
revised stored-PC64 kernel; the earlier run is an intermediate result only.

## Final kernel validation

The read-only review found no production defect and independently verified the
47 precision holdout answers, unchanged echoed inputs and prediction-before-answer
timestamps. Other paired-statistic means retain their original arithmetic.

The final full bank retains 3,895,333/3,910,795 matches (15,462 failed, including
4 unread and 2,064 excluded). The row comparison reports +0/-0 and no changed
actuals among 3,910,791 decoded rows; all input/selection/answer/source/environment
fields are unchanged. This bank has no CORREL, PEARSON or RSQ rows, so it supplies
regression coverage for the other functions, not new correlation evidence.
`cargo xtask check` passes with 909 tests passed, none failed, three ignored and
one not run.

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
