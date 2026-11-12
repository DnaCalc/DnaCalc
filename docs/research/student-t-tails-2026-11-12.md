# Student t tail shape (dc-bc1o), 2026-11-12

Codex on dna-koderbot; DNA Calc code, existing captured inputs and public Excel
answers only. No Microsoft source or disassembly, no restricted source code.

## Predictions before queries

T.DIST.2T(4/3,6) is one ulp high in the captured workbook. The current T tails
form beta coordinates directly as df/(df+x*x) and x*x/(df+x*x). The existing
F tails instead form the smaller/larger ratio and then the paired coordinates.
Mathematically, the two-sided t tail equals the right F tail at x*x, degrees
(1,df). Hypothesis: Excel shares the F tail graph, including that ratio order,
with T.DIST.2T, then halves it for T.DIST.RT and complements for T.DIST CDF.

Freeze independent ordinary/PC64 square inputs before F.DIST.RT primitive
queries, and then freeze all composed target predictions before asking the three
T functions. Include the captured input, degrees 1/2/3/6 and larger, both CDF
signs, plus fresh cases selected where square publication differs. A matching
public identity alone will not count as a product fix; the actual product's F
kernel still needs to match the measured primitive on the selected inputs.

The shared rounded F-tail identity matches 185/224 two-tail and right-tail
answers, and 186/224 signed CDF answers. It fixes the captured two-tail/right-tail
values as a public composition, but fails broadly at degrees 1 and 2 and in
several general-degree cases. It is not an identified universal replacement.
No T production change is made in this batch.

At the captured input, direct and ratio coordinates have identical bits, but
F supplies the smaller coordinate first to BRATIO and uses its complement,
whereas T currently supplies the larger coordinate first. Before new queries,
compare direct, ratio and reciprocal-ratio coordinates with fixed versus
smaller-first orientation. Freeze fresh inputs and coordinates before the beta
queries, then composed predictions before T targets. Rounded public complements
may lose information relative to BRATIO's paired outputs; treat that as an
evidence limit, not a reason to fit coefficients.

Fresh 256-case public-beta orientation probe: direct smaller-first composition
190/256; fixed orientation 126/256. At degrees 1/2, fixed orientation is 45/45,
whereas smaller-first is 22/45. The captured row remains ...7f0 under rounded
public beta compositions, versus Excel ...7f1: those do not expose BRATIO's
unrounded paired complement. Existing T product replay is 361/480 across the
two retained batches, so primitive-pair arithmetic needs direct qualification.

Next prediction plan, before any new Excel answers: use the existing BRATIO
primitive in a temporary native diagnostic to freeze candidate paired-coordinate
results, including both of its outputs. Compare fixed/reversed/smaller-first
orientation, direct versus ratio versus normalise-then-square coordinates,
paired divisions versus forming one coordinate as a complement, and ordinary
versus stored-PC64 arithmetic. No duplicate evaluator or coefficient table is
introduced. The diagnostic is research-only and will be removed before a change
lands; application verification will still go through calc_func::invoke.

## Paired primitive result and next hypothesis

The temporary diagnostic reproduces all 480 baseline product results exactly.
Across 72 coordinate/orientation/precision models the best calibration score is
364/480 (PC64 direct paired fixed), versus baseline 361/480. On the 256 fresh
answers both score 196/256; all orientation alternatives are worse. No wrapper
rewrite is justified. The diagnostic was removed; its source and frozen outputs
are retained as research, not application-test passes.

The captured case reaches BRATIO's bup_140 arm, then bgrat: BUP(3,.5,large,small,20).
The all-shapes-above-1 recurrence arm already uses measured leading factor
x*y*beta_density(x,y,a,b)/a, but bup_140 still selects generic brcmp1/a. New
hypothesis before queries: the same measured prefactor applies when one shape
is <=1, wherever the existing unit-density helper supports the shapes. Reuse
that helper; change no recurrence order, tolerance or coefficient. First qualify
the candidate on retained calibration inputs through calc_func::invoke. If it
improves them, freeze predictions for a new independent batch before asking
Excel; full bank/corpus remain the landing gate.

The prefactor-only candidate gains three and loses one calibration case
(363/480 versus 361/480), and leaves the captured row unchanged. It is reverted;
no fresh holdout is claimed for it. Another documented but unlanded BGRAT finding
already identifies z as stored nu*LN(x), whereas the code still uses alnrel(-y)
when y<=.375. Before new queries, qualify that measured log/product graph in
isolation, leaving the recurrence prefactor unchanged. No constant is changed.

The isolated LN/product candidate gains three and loses no calibration cases
(364/480), with the captured T row unchanged. The one-workbook application corpus
moves 502 ->503 /517, with BINOM.DIST at B6 gained and no loss. Before claiming
that graph, freeze both baseline and candidate invoke outputs on a new batch:
256 large-shape BGRAT-arm t tails and 128 mixed-degree controls, then ask Excel.
The two source versions are built sequentially in this master checkout and the
predictor records each source-diff hash. Restore the candidate after freezing;
do not conflate the unlanded prefactor or orientation experiments with it.

## Rejected BGRAT candidate

The independent holdout gains 12 and loses one of 384 cases (baseline 94,
candidate 105 matches). In the targeted BGRAT group it is 6 ->17 /256; the
128 mixed controls remain 88/128. Lost holdout row 122: x=0x402f3225ddb7cdf6,
df=0x408f400000000000; Excel/baseline=0x35db6ab69a42ae9f, candidate=...e9e.
The targeted workbook gains BINOM.DIST at B6 (517 cells, 502 ->503), not the
captured T tail. Those are intermediate experiment results, not landed passes.

The existing accuracy guard rejects the candidate: BETA.DIST(0.9375,20.45,.75,TRUE)
is 9 ulp off, beyond its unchanged <=5 bound. The prior version is 3 ulp off.
The LN/product candidate is reverted without weakening the assertion or updating
pinned expectations. No BGRAT or T wrapper arithmetic change lands. Full-bank
validation was not repeated for a candidate already rejected by this guard.
The temporary native model diagnostic is also removed. Remaining error lies
in the shared beta/asymptotic machinery; measured z alone does not fix it.

The restored existing BGRAT accuracy guard passes again (1/1). Raw rejected
candidate/holdout/guard evidence: R2 dnacalc-evidence/research/round55/
20261112-fn2-ttest-0619/ (101 files, 197,186,509 bytes for the batch; checksum
copy and one-way check, zero differences). These are not landed T-tail gains.

## Half-shape gamma leaf, before retained-case calibration

The BGRAT remainder calls GRAT1 at b=1/2, where this retained TOMS body
still uses NSWC ERF/ERFC approximations. The main gamma family instead
uses the already-measured shared public ERF/ERFC kernels. Hypothesis: the
asymptotic remainder shares those leaves too. First qualify isolated reuse
(with ordinary versus staged sqrt) on retained T answers and the unchanged
BGRAT accuracy controls. Separately test its prefactor r=b*PoissonMass(b,z),
which avoids two rounded operations in z*GammaPDF(z,b). Keep coefficients,
convergence limits and dispatch unchanged; no production change unless a
strong calibration survives fresh frozen invoke queries and the full gates.

Isolated shared ERF/ERFC reuse improves the 1,120 retained T tails from
651 to 686: 36 gained, one lost. Staged sqrt has no effect on these inputs.
The prefactor-only change has no net gain; the LN(z) variants still violate
the existing beta accuracy guard. Keep only the shared half-shape leaf,
without either rejected change. The captured T tail remains unchanged.

Before new queries, freeze 512 independent mixed-function controls: 256 T
tails (192 large-degree BGRAT and 64 mixed controls), 128 half-shape beta
CDFs and 128 non-half-shape controls. Freeze both ordinary and shared-leaf
native models, requiring the baseline to reproduce invoke on every input.
The isolated candidate must reproduce its model through invoke before any
application claim. No coefficients, branch limits or assertions change.

All 512 candidate invoke results reproduce their frozen native models. Five
focused paired/alias controls and the unchanged BGRAT accuracy guard pass.
The final full bank rejects the change: 3,895,336 ->3,895,335 /3,910,795
matched, two gained and three lost, ten actuals changed. BETA.DIST loses
oc307 and oc316 (build-20131); FDIST loses fd-var-ref-v3-034-0002
(work/w109/G3-06-ftest-variance/answers-fdist-variance-refinement-discovery-v3.json).
TDIST gains two and loses none. All 3,910,791 decoded reading/input/answer/
source/environment identities are unchanged.

The source, focused candidate test and rejected status are reverted exactly
to the landed commit. No pin or Excel reading was refreshed; no full corpus
run is claimed for a candidate rejected by the bank. The restored existing
BGRAT guard passes again. Fresh holdout loss 52 and all three bank losses
remain in the raw transition records. This is rejected research, not landed
parity or a new assertion allowance.

## Remainder quotient store, before retained-case calibration

Public ERFC for root>=1/2 is store(E*F), with both E and F independently
measured existing leaves. BGRAT then forms j=Q/r. Test whether the remainder
instead stores E*F/r once, or stores E/r before multiplying F (and its
symmetric alternative); also distinguish PC64 division of the rounded Q.
Keep E's stored square/exponential, F's coefficient table and every BGRAT
coefficient/iteration unchanged. The three newly lost bank rows and all
retained 1,120 T tails are calibration, not fresh evidence. Require a fresh
frozen holdout and the full gates if a quotient shape is promising.

The quotient-store calibration is rejected: the best remains 686/1,120,
and every proposed graph keeps both BETA bank losses; only one weaker
association recovers the FDIST loss. No quotient change or fresh shape
holdout follows. Before new diagnostic rechecks, freeze the two gained and
three lost bank rows' complete inputs/old/new/retained answers, then ask the
current Excel service. This distinguishes real losses from possible build
differences without changing any retained bank answer.

Further rejected-model evidence: dnacalc-evidence/research/round58/20261112-fn2-rejected-0830/,
64 files, 120,226,186 bytes; checksum copy and one-way check zero differences.
These records do not add a landed pass or change any Excel answer.

The quotient and fresh bank-recheck records are in
dnacalc-evidence/research/round59/20261112-fn2-fshape-0907/
(93 files, 16,602,670 bytes for the research batch; checksum copy and
one-way check zero differences). Excel 20527 confirms all three real bank
losses and both gains; the rejected shared-leaf change remains reverted.
