# Gamma normalization in distribution densities (dc-bc1o), 2026-11-12

Codex, dna-koderbot. Existing DNA Calc code, mathematical distribution identities
and public Excel outputs only. No Microsoft source or disassembly.

## Predictions before queries

The captured F and Student-t PDFs use legacy Lanczos gamma values, unlike the
measured public GAMMA/GAMMALN surfaces. Test whether those public primitives
explain the density before modifying a shared normalization. Compare direct
gamma quotients and powers with log-gamma exponent forms, ratio versus p/q
coordinates, and exponentiating normalization together/separately. Retain all
mathematical coefficients and pi unchanged. Compare ordinary and stored-PC64
arithmetic. Freeze primitive arguments before querying; freeze exponent inputs
before EXP and all composed predictions before new F.DIST/T.DIST answers.
Include both captured roots and independent odd/even degrees. A weak fit rejects
the public composition and does not justify coefficient fitting.

The fresh public-gamma compositions are rejected: best F PDF 6/192, Student-t
PDF 17/192. A logarithmic Student-t form matches the captured row but only
14/192 overall, so it does not land. Every numeric/boolean value is echoed
unchanged; the service omits the boolean transport-only mode=cell metadata.
The initial strict whole-object echo check rejected that omission, after which
semantic value checks verified all inputs without repeating or changing F calls.

## Isolate the Student-t normalizer, before queries

Before identifying more normalization arithmetic, separate its x dependence.
Measure T.DIST(0,v,FALSE) and freeze PDF predictions using that measured constant
and public POWER(1+x*x/v, +/- (v+1)/2), multiply versus divide, ordinary and
stored-PC64 stages. Also compare the gamma/log-gamma exponent differences
carried inside one EXP. Existing 192 targets are retained calibration; a strong
spatial shape would require independent frozen targets and a production
normalizer, not a table of measured answers.

The measured zero-density normalization followed by the rounded base power also
fails (16/192). Before another primitive call, test the reciprocal coordinate
p=v/(v+x*x), its 1/(1+x*x/v) rival, and the corresponding positive power p^a
and exponential a*LN(p). This isolates the coordinate graph without treating
the zero-density readings as an implementable normalizer. Retained target
answers remain calibration, not fresh holdout evidence.

The reciprocal-coordinate replay is weak too (best 22/192); it does not fix the
captured PDF. A different, derived normalizer avoids cancellation of log-gammas.
With a=v/2, b=a+1/2, Stirling error s and deviance D(a,b)=a*ln(a/b)+b-a,
log(C_v)=s(b)-s(a)-D(a,b)-ln(sqrt(2*pi)). This follows by subtracting the
defining Stirling expansions; Loader (2000), already cited by discrete_dist.rs,
provides the existing measured s and D primitives. Test this identity with those
primitives, not new coefficients. Spatial equivalents are exp(t-b*ln(1+r)) /
sqrt(2*pi), or exp(t-a*ln(1+r))/sqrt(2*pi*(1+r)), r=x*x/v.
Compare existing log1p/log1p-times and ordinary/stored operations on retained
zero and spatial answers as calibration. The temporary standalone native model
includes the existing numeric module by path, avoiding a second Cargo invocation
while the corpus runs; it is not a product result. A successful shape still needs
fresh frozen queries and actual invoke qualification.

The derived zero-density identity matches 38/40 distinct degrees with the
existing inverse-sqrt(2*pi) factor; the spatial graph reaches 114/192 with
separate sqrt(1+r) normalization and accurate log1p. The combined denominator
scores 79/192 (its diagnostic was emitted three times per input; deduplicate
before counting). Before fresh targets, isolate equivalent exponent subtraction
orders and placement of the fixed inverse-sqrt(2*pi) factor, reciprocal-square-
root versus division, and the existing running-exponential correction. These are
retained-case calibrations with unchanged coefficients. Do not select a formula
merely because it fixes the captured root.

Equivalent factor placement separates a much stronger spatial shape:
exp(t-a*log1p(r)) * (1/sqrt(2*pi) / sqrt(1+r)), with the quotient formed
before the multiply, reaches 165/192; distributing that quotient only reaches
114/192. Both remaining zero misses are v=1/2, which already have separate
production branches; all 38 zero controls for v>=3 match. The captured spatial
row is still one ulp high. Before fresh queries, test stores in this now-supported
shape: quotient input, logarithm/product, exponent, exp result, root and fixed
factor division. Use the existing numeric backend in a temporary research copy
with path-bound child modules and only an exp_ext visibility bridge. Keep all
original numeric bodies and coefficients unchanged. Compare public accurate
log1p with unspilled 1+r/FYL2X as an explicit rival. A high calibration score must
still survive a new independent frozen holdout and actual invoke route.

Store-site calibration does not exceed 165/192: the fully stored graph remains
best, while unspilled quotient/log/exp variants lose matches. Residuals reach
19 ulp where a large degree multiplies a one-ulp logarithm difference. Before
new targets compare conventional compensated log1p identities using the public
LN leaf: Kahan's ln(1+r)*r/((1+r)-1), with its three evaluation orders, and
ln(s)+(r-(s-1))/s with compensated two-sum recovery. These use no new constants.
Retain the same Stirling normalizer and factor-before-multiply spatial graph.

Compensated-LN calibration reaches 162/192; the existing pure-Rust libm log1p
reaches 164/192 and fixes the captured row, versus the Arm-based helper's 165/192
and one-ulp captured miss. This is not enough to select the leaf by one case.
Before querying again, freeze all three leaves (Arm helper, libm, compensated
public LN) and two exponent groupings on a new independent batch: ordinary and
large degrees with informative nonzero densities, zero/df=1/2 controls, and
cases selected solely by candidate disagreement. Retain the existing df=1/2
branches in every model. Freeze the unchanged current gamma-based model too;
standalone predictions remain models until reproduced through invoke. Include
out-of-range df refusals and both signs of x. No leaf coefficients are tuned.

## Independent shape holdout and planned product qualification

The 521-call holdout selects no fitted constants. Ordinary: old 2/256 versus
Kahan-additive 228/256; large degrees: 1/128 versus 102/128; zero/df=1/2 controls:
24/64 versus 64/64; disagreement-selected: 0/64 versus 31/64; domain probes:
0/9 versus 3/9. Overall 27 ->428/521, 401 gains and zero losses. The Arm leaf
scores 419, libm 417. The grouped-exponent variants are worse (best 398).
The baseline model is reproduced bit/error-exactly by invoke on all 521 inputs.
Candidate results remain mathematical models until product replay.

The domain probes also reveal existing gaps: PDF df<=0 returns #DIV/0! in Excel,
the common validator's 1e10 cap is not Excel's PDF limit, and an overflowing
x*x returns #NUM!. Do not claim these unrelated admission rules are fixed.
Keep the existing df validator and df=1/2 branches for this arithmetic change.

Before a final independent query, freeze baseline/candidate invoke predictions
on a new 512 admitted calls (320 ordinary, 128 large degrees, 32 zero, 32
existing-special controls). Retain all 521 earlier inputs in the same predictor
file to verify the standalone candidate. Commit the candidate only if those
invoke results reproduce its model and the final full-bank/corpus/check batch
shows a clear net gain, with all losses listed. The Kahan leaf is a numerically
sound call-site graph with a strong measured improvement; unresolved last bits
remain explicit rather than claimed as complete identification.

The candidate reproduces all 521 standalone model results through invoke. On
512 further calls frozen through both application versions before querying,
33 ->451 match, 418 gained and zero lost: ordinary 1 ->274/320, large degrees
0 ->113/128, zero 0 ->32/32, retained special branches 32 ->32/32. The remaining
61 answers are failures, not approximate passes. Production qualification keeps
the original admission rules and reuses the single deviance/Stirling primitive
and the existing inverse-sqrt(2*pi) constant.

## Derived F saddle-point graph, before calibration

The same defining Stirling expansions give an independent F-density form.
For a=m/2, b=n/2, N=a+b, p=m*x/(n+m*x), q=n/(n+m*x), let
E=s(N)-s(a)-D(a,N*p)-s(b)-D(b,N*q). Then f(x)=exp(E) *
sqrt(a*b/N) / (sqrt(2*pi)*x). This is the binomial mass at a of N with its
normalizer simplified symbolically before rounding, unlike the earlier rounded
public-binomial compositions. Compare equivalent A/B/C/D recurrence masses
with their own raw exponent and normalization outside EXP; direct versus ratio
coordinates; ordinary/PC64 and factor placements. Reuse the existing helpers
and retained F answers as calibration. No scale is fitted; a promising graph
requires fresh frozen application predictions before another target batch.

The derived F saddle-point replay is rejected too (best 14/192, captured row
still wrong). No F code change lands. Five focused Student-t invoke controls
(captured, truncation/sign, zero, large degree, logarithm discriminator) pass on
both the native and software-x87 backends. Only the Student-t PDF candidate
continues to full validation; shared function bodies and coefficients remain
unchanged apart from exposing their existing crate-private interfaces.

## Existing incomplete-beta prefactor, before calibration

One untested F shape uses the existing TOMS 708 brcomp prefactor directly:
B_c(a,b,p,q)=p^a*q^b/B(a,b), hence F PDF=B_c(a,b,p,q)/x. Its arithmetic
is distinct from both the public beta density and the binomial saddle point.
Reuse that existing function and its dependency closure without changing its
coefficients. Compare direct/ratio coordinates, accurate versus rounded
complements, orientation and final division precision on retained answers.
The analogous Student-t identity B_c(v/2,1/2,p,q)/abs(x) is a diagnostic only;
its zero limit would require a separate derivation. Reject a weak fit and do
not alter coefficients to save it. No new Excel target is claimed in calibration.

The existing brcomp prefactor is also rejected (F best 9/192, Student-t 20/192);
its coefficients and implementation are unchanged.

## Landing evidence

The final full bank remains 3,895,333 /3,910,795 matched, 15,462 failed
(4 unread, 2,064 excluded). Streaming comparison verifies all 3,910,791 decoded
rows with unchanged inputs/selection/answers/sources/environments: +0/-0,
zero actuals changed. The bank has no T.DIST PDF rows; its neutrality is a guard.
Oracle corpus: 208,196 ->208,197 /212,841 formula cells, +1 T.DIST/-0 across
all 212,217 recorded cells. The gained Sheet1!B81 actual equals the baseline
live-Excel bits ...a194. Failures 4,644: 3,133 mismatched, 771 refused, 545
failed/missing, 73 external, 14 circular, 108 awaiting capture. The 516 timeout
cells remain failed/missing. No fresh or recorded row is lost by this change.

Check: workspace and wasm checks passed; 730 tests passed, none failed,
three ignored and one not run in 13 harnesses (115.721 seconds). The focused
PDF controls also pass with the software-x87 backend. Remaining 61/512 final
holdout failures, six of nine initial domain probes and other statistical roots
stay open; no accuracy assertion or recorded expected value was weakened.

Raw evidence: dnacalc-evidence/research/round56/20261112-fn2-tdensity-0657/,
114 files, 178,812,671 bytes. Checksum copy and one-way check: zero differences.
No rejected F/normal/IRR/hypergeometric model is included in production.

## F density via the gamma ratio, before retained-case calibration

The binomial saddle point rounded both probability coordinates. Another
identity works directly in x: for a=m/2,b=n/2,N=a+b,h=a/b,
f(x)=sqrt(a)/sqrt(2*pi)*exp(s(N)-s(a)-s(b)
 +(N-1/2)*log1p(h)+(a-1)*LN(x)-N*log1p(h*x)).
Derive it by subtracting the defining Stirling expansions; no constants
are fitted. Test stored/public-compensated/internal log1p products,
exponent groupings and factor placement on the retained 192 F densities,
with current invoke as control. This separates ratio normalization from
the earlier rounded-p saddle point; a weak fit rejects it without new
queries or production changes.

The direct gamma-ratio calibration is rejected (best 11/192, baseline 1/192;
no model fixes the captured F density). Before new F questions, freeze 512
fixed-degree (6,8) density inputs and the current invoke/exact-mathematical
predictions. Distinguish a single stored exponential from ratios of rounded
powers by the phase of LN(answer/K) on the exponent's binary64 lattice.
Declared K families: one; the fixed beta/gamma ratio with/without h^a;
the x-polynomial numerator divided by beta; sqrt(a)/sqrt(2*pi); and the
simplified binomial factor sqrt(a*b/N)/(sqrt(2*pi)*x). Compare mathematically
equivalent stored factor placements without changing any coefficient.
Use the large-|exponent| rows so output rounding is small relative to the
exponent lattice. A phase concentration identifies a graph family only,
not a product pass; a diffuse phase rejects that one-exponential family.

The new exponent-grid result supports the saddle scale family on this fixed-degree slice: with
K=sqrt(a*b/N)/(sqrt(2*pi)*x), 362/388 eligible rows lie within 0.1 of
an exponent ulp and the median phase is 0.034 ulp. The other declared
factors have diffuse phases (about 20% within 0.1, median about 0.25).
The actual baseline matches 5/512 and the exact mathematical result only
29/512; neither is claimed as an identified Excel program.

Before retained calibration, exploit the exact linear cancellation in the
two deviances: E=s(N)-s(a)-s(b)+a*LN(x)-N*LN((a*x+b)/N). Compare direct
ratio and reciprocal logs, and log1p(a*(x-1)/N), with stored versus internal
log products and exponent associations. Use existing fixed Stirling errors,
LN/EXP and the saddle scale; no coefficient or threshold is tuned. Retain
the rounded-mean deviance form as a comparator. A strong graph requires
fresh degrees and frozen invoke validation, not just the fixed (6,8) slice.

The direct exponent forms remain weak: best 105/512 on the fixed-degree
slice and 10/192 on mixed degrees. The reciprocal-log form reproduces the
captured answer but is rejected by the broader fit. The phase evidence for
the saddle scale is retained independently of that rejection.

Before more retained calibration, distinguish the mean-coordinate construction
and the interleaved Stirling/deviance order used by the existing binomial
exponent. Compare N*(a*x/den), (N*a*x)/den, a*(N*x/den), a*x/(den/N),
paired reciprocal ratios and a shared displacement, with both interleaving
orders and grouped negative controls. Keep the existing deviance body and
Stirling errors untouched; no coefficient fit or fresh target is claimed.

The ten mean constructions/interleavings are also weak (best 11/192),
so none is selected. Next isolate the scale rather than attributing all
misses to the exponent. For the 388 large-exponent fixed-degree rows,
round LN(answer/K) to its nearest binary64 exponent using the approximate
saddle K, freeze those exponents and native EXP predictions, then ask EXP.
On retained F answers, compare fixed reciprocal-multiply versus sqrt/divide
normalizers, placement of x and 2*pi inside/outside sqrt, and final product
versus quotient stores. These are factor-shape calibration predictions only;
the original F targets were already observed. No constant is tuned.

All 388 frozen EXP primitives match the native kernel. With those exponents,
the best declared scale is EXP(E)*sqrt(a*b/(N*2*pi))/x (246/388), stronger
than forming sqrt(a*b/N)*INV_SQRT_2PI, but still incomplete. Before further
calibration, isolate its x87 stores: the denominator product, ratio, sqrt
and product/division publication, retaining versus storing each intermediate.
Contrast the double 2*pi with the documented x87 ROM pi only after fixing
these graph families; no fitted pi or empirical scale is introduced.

Retaining the candidate scale's x87 intermediates raises the best to
260/388, still too weak. Before treating its mathematical coefficient as
identified, inspect the implied constant C=answer*x/EXP(E) across those
rows and test its adjacent lattice loads under the declared C/x product
and quotient graphs. This is an explicit constant-read diagnostic after
the scale-family phase test, not an empirical constant allowed into
production. A diffuse C or weak best load rejects the factor graph at
this precision; no degree-specific table will be introduced.

The implied C loads cluster at the mathematical saddle constant and its
neighbours, but no adjusted binary64 load improves 260/388. No fitted
constant is selected. Before more calibration, test the missing store: EXP
itself may return a running PC64 value to the quotient rather than first
publishing binary64. Compare a high-precision mathematical exponential
with stored/running scale and final product sites as a model; a promising
result must then be reproduced by the existing native exponential chain
with its final store deferred, not by a second production exponential.

Running mathematical EXP improves only to 274/388 and remains an incomplete
scale model. Another existing graph accounts for an x-dependent last-bit
scale: beta/Poisson densities put the logarithmic normalizer inside the
compensated exponential. Before retained calibration, combine the unchanged
binomial-style E with normalizer terms -LN(a)/2,-LN(b)/2,LN(N)/2,
LN_SQRT_2PI,LN(x), using the existing excel_exp_running/compensated helpers.
Compare the natural term orders and earlier mean-coordinate constructions;
no coefficient or new exponential is introduced. Only a strong broad fit
would justify fresh frozen-degree targets and application qualification.

The logarithmic-normalizer calibration is rejected too (best 15/192; no
captured match). Before attributing the remaining factor errors to arithmetic,
compare the repository's existing CRT EXP countermodel with the measured public
EXP at the inferred exponents. Internal distribution code need not call the
worksheet exponential. This reads no runtime source and changes no primitive;
only a substantially stronger factor fit would support that call-graph choice.

CRT EXP agrees with the public EXP on 381/388 and does not improve the
factor fit; it is rejected as the explanation. One remaining exponent-store
ambiguity has not been qualified: its four Stirling/deviance subtractions
may run at PC64 until the argument store rather than publish after each
subtraction as BINOM does. Before calibration, vary only those store sites
under the existing interleaved/swapped/grouped term orders and earlier mean
constructions, with all input pieces and coefficients unchanged. Reject a
weak broad fit; no normalizer constant is fitted.

The exponent-store sweep is rejected (best 12/192, no captured match).
Before new structural questions, freeze 60 tiny-x controls at degrees (6,8):
five adjacent floats around each of 2^-k, k=20..400. The exact limiting
coefficient is f(x)/x^2 ->25.3125. Direct powers should give a nearly fixed
coefficient once 1+0.75*x rounds to one; an exponential of a stored logarithm
should retain a growing log-sized rounding pattern. Freeze current invoke
and exact-rational mathematical predictions before asking F.DIST. This
measures the family, not new fitting coefficients or a production shortcut.

The tiny-x probe matches neither baseline (0/60) nor exact mathematics
(1/60). At k>=60 the normalized coefficient spreads from roughly -593
to +198 ulp around 25.3125 rather than staying constant. At k=400 Excel
returns zero for all five inputs even though the final mathematical
density is normal: this supports a pre-scale exponential with the a*LN(x)
term, whose underflow the later 1/x cannot recover. No small-x shortcut
or strengthened pass claim follows.

Before one final retained shape test, use the inferred exponent words only
where the scale-phase distance is <0.1 ulp. For each earlier variable-term
construction V, intersect the rounding intervals of E0+V across inputs;
a single shape-only E0 must satisfy them if that graph is right. Compare
that constrained constant with the existing Stirling sum and constants
derived from public GAMMALN/LN; never introduce a fitted degree table.
A weak common interval rejects the variable graph without new targets.

The interval test rejects those variable graphs: at most 172/361 high-phase-
confidence rows share any E0 interval; the existing Stirling/public-GAMMALN
constants cover only about 110. No fitted degree constant is selected. The
phase evidence and tiny-x underflow are structural clues, not complete
identification or landed parity. All F-density experiments remain outside
production; the existing captured F PDF mismatch stays open.

Later structural F-density evidence: dnacalc-evidence/research/round59/
20261112-fn2-fshape-0907/, 93 files, 16,602,670 bytes; checksum copy and
one-way check zero differences. Large JSON/log records are losslessly
gzipped; sources and all frozen inputs/observations are retained. No F
implementation or expected value was changed by these experiments.
