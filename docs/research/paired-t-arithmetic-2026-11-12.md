# Paired t-test arithmetic (dc-bc1o), 2026-11-12

Codex, dna-koderbot. Existing DNA Calc code, captured workbook inputs and public
Excel answers only; no Microsoft source or disassembly.

## Predictions before queries

The captured T.TEST(F2:F10,G2:G10,1,1) is two ulp high. Separate the paired
statistic from its probability: compare centered two-pass differences with
uncentered sum-of-squares and normalization before/after square root. Retain
n-1 degrees of freedom. Vary plain versus stored-PC64 arithmetic at subtraction,
mean, centered accumulation and final scaling. Public T.DIST.2T at each frozen
statistic provides a candidate probability, halved for the captured one-tail
case. Freeze all candidate statistics before these primitive calls, then freeze
all composed predictions before the new T.TEST answers. Include independent
pairs and an exact-dyadic control population. No fitted coefficients or changes
to the probability kernel are justified by a failed public composition.

The public rounded t-statistic compositions are weak (best 27/129), although
three algebraic forms fix the captured row. That is insufficient to select one.
Next qualify the actual paired beta outputs with frozen retained inputs as
calibration: mean of differences versus difference of means; centered differences
versus separate covariance sums; t squared before/after square root; direct,
ratio and normalized coordinates, independently formed versus rounded-complement
pairs. Reuse the existing beta primitive and compare orientations. A native
private diagnostic will reproduce the current product as its control and be
removed. Only a strong calibration would justify fresh predictions and queries.

The native current-route control reproduces the captured ...091f and scores
17/129. Keeping its statistic and changing only y to 1-z scores 64/129, with
50 gains and 3 losses, and fixes the captured ...091d. The best variance variant
scores 68/129, too close to choose a new variance rule. Before new targets,
measure public BETA.DIST at the already frozen statistics: its one-coordinate
CDF necessarily uses 1-z. If the same rounded beta call explains the test,
this separates shared kernel errors from a wrong statistic. Retained target
answers are calibration only, not an independent holdout. Remove the temporary
native diagnostic before qualification of the isolated complement change.

The public beta CDF at the existing direct statistic matches 109/129 retained
T.TEST answers; changing to the ratio coordinate matches 74/129. This separates
the private-kernel gap from the statistic: keep the existing mean and variance,
and qualify only the rounded complement y=1-z. Freeze baseline and candidate
invoke outputs on 384 new paired/equal-variance/unequal-variance calls (128 each,
both tail counts, n=2/3 and continuous/dyadic controls). Then ask Excel and
compare all losses explicitly. No variance formula is selected from calibration.

## Independent invoke qualification

On 384 new calls the isolated complement change moves 39 ->195 matches:
167 gained, 11 lost, 28 matched by both and 178 by neither. Paired: 17 ->64/128;
equal variance: 6 ->62/128; unequal variance: 16 ->69/128. Inputs were echoed
unchanged; both product versions were frozen before the Excel request.
The loss IDs in seed 0xDC778826 are 15, 64, 92, 109, 137, 149, 156, 232, 287,
288 and 358; deviations are respectively 6, 1, 2, 1, 4, 2, 2, 1, 1, 1 and 2 ulp.
Exact inputs and bits are in ttest-holdout-losses.json. No loss is hidden by a
rounded display or counted as a pass. Current shared beta and statistic gaps
remain. Four focused invoke cases (captured plus fresh controls for all three
test types), each through T.TEST and TTEST, pass. Full bank/corpus/check pending.

## Landing evidence

Full bank: 3,895,333 /3,910,795 matched, 15,462 failed (4 unread, 2,064 excluded),
unchanged. Streaming comparison verifies all 3,910,791 decoded rows and all input,
answer, environment and source identities; +0/-0, zero calculated values changed.
The bank has no T.TEST/TTEST rows, so its unchanged count is a guard, not new
coverage. Oracle corpus: 208,192 ->208,196 /212,841 formula cells; failures
4,649 ->4,645 (3,134 mismatched, 771 refused, 545 failed/missing, 73 external,
14 circular, 108 awaiting capture). All 212,217
recorded cells were compared, with four gains and no losses: captured Sheet1!B70
and catalogue prefix_metadata A227, A234, B234. Every gained actual bit equals
the baseline live-Excel observation. The 516 timeout cells and 108 awaiting
capture remain failures. Check: 729 passed, zero failed, three ignored, one not
run in 13 harnesses; workspace and wasm checks passed (117.085 seconds).

Raw evidence: R2 dnacalc-evidence/research/round55/20261112-fn2-ttest-0619/,
101 files, 197,186,509 bytes. Checksum copy and one-way check: zero differences.
