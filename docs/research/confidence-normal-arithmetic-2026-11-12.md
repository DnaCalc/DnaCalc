# CONFIDENCE.NORM arithmetic (dc-bc1o), 2026-11-12

Implementer: Codex, dna-koderbot. Sources: DNA Calc code and Excel outputs
(tier 1). No Microsoft source, disassembly or restricted source.

The captured case has alpha=0.13333333333333333, sd=6.6, n=44 and expects
0x3ff7e5a0e8d17763; current code computes ...762. Current code takes the upper
inverse at 1-alpha/2, multiplies by sd, then divides by sqrt(trunc(n)).

## Predictions before new queries

H1: Excel uses the lower-tail inverse at alpha/2, negated, avoiding the loss
from subtraction near 1. H2: it uses upper 1-alpha/2 as current code does.
Each has three scaling orders: (z*sd)/sqrt(n), z*(sd/sqrt(n)), (z/sqrt(n))*sd.
A stored PC64 square root is a later candidate, only if basic ordering fails.
At alpha=1e-20, H1 should give a finite answer whereas H2 gives #NUM! because
its rounded probability is 1. Test independent alphas, deviations and sizes.
First measure both public inverse values, freeze all resulting composition
predictions, then ask CONFIDENCE.NORM and legacy CONFIDENCE on those arguments.
Inputs and primitive queries are generated with deterministic seed 0xDCBC1026.

## Identification result

121 cases, including the captured case and 120 generated cases. Nine have
alpha=1e-20, where upper probability rounds to 1; both confidence functions
and the upper inverse return #NUM!. Remaining cases are compared numerically.
The upper inverse times (sd/sqrt(trunc(n))) matches every numeric answer.
The same upper inverse with multiply-then-divide matches fewer answers;
the lower-tail candidates fail the zero-complement error controls as well.
No coefficient or threshold changed. Initial script scores use missing bits
for the upper error cases; count those as explicitly matching #NUM!, never
as numeric matches. Raw inputs, primitive answers, frozen predictions and
function answers: ~/codex-runs/fn2/confidence-probe-*.json.

Product-route check: changing only the scaling order still returns ...762 on
the captured row. The primitive inverse supplied by Excel differs from DNA's
existing generic central/upper-seam inverse. This is not a fixed corpus case.
Before further queries: test the published Wichura AS241 rational shapes using
its fixed coefficient tables and binary64 versus stored-PC64 Horner sites.
Do not tune coefficients until a shape is selected; keep the scaling finding
separate from the unresolved inverse.

Published fixed coefficients: Michael Wichura, AS241 (1988), pp. 478, 482–483,
https://csg.sph.umich.edu/abecasis/gas_power_calculator/algorithm-as-241-the-percentage-points-of-the-normal-distribution.pdf
(tier 1). Use the mathematical coefficient values; no Fortran source is ported.
The central candidate is q*A(r)/B(r), q=p-1/2, r=0.180625-q*q. Freeze
binary64 predictions on independent central probabilities and the upper seam,
then ask Excel. A poor fit rejects this shape rather than justifying constants.

## Rejected candidate and disposition

The published central AS241 binary64 shape matches only 861/4096 independent
central answers, and 1/1024 in the upper seam. Python NormalDist (an independent
AS241 composition) matches 861/4096 and 103/1024 respectively. This rejects a
wholesale AS241 substitution; do not tune constants to rescue a failed shape.
Raw: ~/codex-runs/fn2/normal-central-{predictions,answers}.json.
The scaling-only production candidate failed the exact captured-case test
(confidence-test.log), so it and that new unpassing test were removed before
landing. Existing tests were not weakened. No CONFIDENCE change lands, and
dc-bc1o stays open. The measured composition is useful once its inverse is fixed.
Read-only review independently confirms 112 numeric + 9 explicit error controls,
and that a primitive identity alone does not prove product-function exactness.

## A new inverse hypothesis, before queries

AS241 central was rejected. The existing kernel instead starts with Acklam's
rational approximation and takes one Newton step using a libm ERFC residual.
New hypothesis: the Newton shape is correct but Excel's own ERF/ERFC residual
is needed. Freeze an independent seed 0xDCBC2026 and the current rational initial
points. Ask public ERF.PRECISE/ERFC.PRECISE and NORM.S.DIST at those points,
then freeze Newton compositions before asking NORM.S.INV on the new probabilities.
Compare x/sqrt(2) versus x*(1/sqrt(2)), signed ERF versus complementary ERFC
residuals, and the public rounded CDF residual. A poor fit rejects this shape;
no coefficient tuning is authorised by a poor fit.

The fresh Newton experiment also rejects that replacement shape: the best
candidate matches only 230/1024 central answers and 95/256 upper-seam answers.
The division-versus-multiplication variants and Newton-versus-Halley step do not
rescue it. Raw: normal-newton-{inputs,primitives,predictions,answers}.json.
No inverse change is made. The current inverse and the compound distribution
functions therefore remain open; neither rejected numerical shape lands.

## Accuracy control, frozen before a fresh query (04:18 UTC)

Before selecting another approximation, test whether Excel's central inverse is
simply the correctly rounded mathematical quantile. Generate 256 new central
probabilities from seed 0xDCBC3026. Use Decimal Newton iteration with the defining
convergent erf power series and 80/100-digit agreement to freeze mathematical
quantile predictions, then ask NORM.S.INV. This is a diagnostic, not a proposed
production algorithm. A poor match rejects accuracy alone as a replacement shape.

The high-precision mathematical predictions match 60/256 fresh Excel answers.
The 80- and 100-digit calculations agree on every published binary64 result.
Thus mathematical accuracy alone does not identify this Excel computation; an
accurate inverse substitution would change many Excel results. No change lands.
Raw: normal-accuracy-{predictions,answers}.json and normal-accuracy-probe.py.

## Fixed-coefficient precision hypothesis

The earlier AS241 test used ordinary binary64 Horner arithmetic. Before new
queries, distinguish that from stored-PC64 operations, running-PC64 Horner,
once-per-Horner-step stores and a fused-operation countermodel. Keep every
published coefficient at the same binary64 value; do not tune coefficients.
Vary the q/r stage precision and the three final multiply/divide orders as
explicit graph candidates. First replay 64 retained central answers, with a
control that the ordinary model exactly reproduces the earlier frozen AS241
predictions. A poor fit rejects these precision graphs too; only a strong
calibration result would justify a fresh frozen holdout query.

The 180 fixed-coefficient precision graphs are rejected on calibration: best
17/64 versus ordinary Horner 11/64, with the ordinary control reproducing all
previous frozen AS241 bits. No coefficient was changed and no new inverse query
is claimed. The required arithmetic shape is still unidentified.

## Published decimal constant precision, before calibration

The preceding sweep kept coefficients and 0.180625 at their binary64 values.
One remaining shape ambiguity is source-literal precision, not fitted digits:
compare those same published decimal strings rounded to 64 significand bits,
and exact decimal constants as a negative control, including the rational
0.180625 at binary64, PC64 and exact decimal precision. Reuse all prior
arithmetic graphs and retained 64-case calibration. No decimal digit is changed
or selected from Excel outputs. Require an independent frozen holdout if a
published-constant precision shape fits strongly; otherwise reject it.

The published-decimal precision sweep is rejected too: best 20/64 over 1,620
graphs, versus ordinary 11/64; the baseline control again reproduces every
previous frozen AS241 value. All published decimal digits remain unchanged.
No fresh holdout or inverse implementation change is claimed from this sweep.

Later precision-calibration evidence: R2 dnacalc-evidence/research/round55/
20261112-fn2-ttest-0619/ (101 files, 197,186,509 bytes for the batch; checksum
copy and one-way check, zero differences). No normal inverse change landed.

## AS241-seeded refinement, before queries

The failed Newton batch started with Acklam, not an already last-bit AS241
point. Test that remaining shape ambiguity with the same published AS241
coefficients: ordinary and fully stored-PC64 central initial values, then one
Newton/Halley step from public CDF/ERF/ERFC and PHI. Compare divide-by-sqrt(2)
and multiply-by-inverse-sqrt(2) arguments and stored versus ordinary correction
arithmetic. Include the captured probability as calibration plus 256 new central
and 64 new upper-seam probabilities. Freeze initials before primitive calls and
all correction predictions before targets. Count zero-residual cases explicitly:
if CDF(x0)=p but Excel's inverse differs, a rounded-CDF Newton cannot fix them.
No coefficient or threshold change is inferred from a weak fit.

AS241-seeded refinement is rejected: best 57/256 new central and 30/64 new
upper-seam answers, and no candidate fixes the captured probability. On 139/256
central cases the public CDF residual is exactly zero while the initial inverse
still differs from Excel, ruling out that rounded-CDF correction for those rows.
No inverse changes or coefficient adjustments land.

Later evidence: R2 dnacalc-evidence/research/round56/20261112-fn2-tdensity-0657/
(114 files, 178,812,671 bytes for the batch; checksum copy and one-way check,
zero differences). No additional implementation change is claimed here.

## Algebraically transformed published central table, before calibration

One remaining arithmetic shape is the polynomial basis: evaluate the same
published AS241 rational after exactly expanding A(0.180625-q*q) and B at
that argument into powers of q*q. Contrast unnormalized and constant-term
normalized denominators, q versus 2*q, and binary64 versus stored-PC64
coefficients/arithmetic. Coefficients are derived solely by binomial expansion
of the published decimals, without fitting any digit to Excel. Use retained
calibration first; a weak fit rejects this basis too.

The exact polynomial-basis expansion is rejected on retained calibration too:
best 37/128 across the declared basis/scaling/precision graphs. No coefficients
were fitted to Excel and no fresh normal-inverse query or implementation
change follows from this weak fit.

Further rejected-model evidence: dnacalc-evidence/research/round58/20261112-fn2-rejected-0830/,
64 files, 120,226,186 bytes; checksum copy and one-way check zero differences.
These records do not add a landed pass or change any Excel answer.
