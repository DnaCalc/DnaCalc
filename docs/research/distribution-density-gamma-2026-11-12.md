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
