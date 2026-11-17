# Chi-square inverse stages (dc-bc1o), 2026-11-12

Codex on dna-koderbot; public Excel observations and existing repository code.
No Microsoft source or disassembly.

Before new queries, freeze current GAMMA.INV at unit scale for 256 new
probability/shape pairs, its adjacent lower/upper floats, and selection of the
nearest public forward-CDF lattice point. Include the captured p=2/15,a=2.5
case separately. Ask GAMMA.INV and GAMMA.DIST at adjacent points, then
qualify CHISQ.INV against the measured unit-root times two. A mismatching
forward surface must be distinguished from root endpoint publication; do not
move the result one ulp solely because that fixes a captured example.

The native model reproduces invoke on all 257 controls. The 256 fresh inverses
match current 85, lower 61, upper 34, nearest-forward 99. The public forward
CDF matches only 481/771 adjacent points. At the captured case, all three
forward points match Excel and current CDF(root) equals p exactly, but Excel
chooses the next root above. Nearest-forward leaves this captured case wrong.

For retained-case accuracy calibration, compute mathematical roots only for
the 128 integer/half-integer shape controls, using the defining convergent
incomplete-gamma series, factorial/half-factorial normalization and Decimal
Newton at both 80 and 100 digits (all rounded roots agree). They match 66/128
versus current 43/128, including the captured point, but are far from a full
Excel program. No solver change follows from mathematical accuracy alone.
The first forward request mistakenly sent unwrapped float values; the service
refused it. The corrected request reuses the original frozen predictions.

## Unrounded series residual, before retained-case calibration

The gamma series publishes store(PoissonMass(a,x)*series). An inverse could
subtract p before that final store and therefore resolve a public-CDF plateau
without changing any public CDF. Isolate that hypothesis: reuse the existing
stored series factors, but form PC64(mass*sum-p) with the existing running
multiply/add primitive. Outside the Taylor arm retain the current CDF residual.
Compare the crossing and nearest-residual lattice points on retained cases,
with the ordinary residual required to reproduce invoke. This changes no
coefficient, convergence limit or public gamma calculation. A weak result
rejects the hypothesis; a strong one needs fresh frozen application queries.

The unrounded-series residual is rejected on calibration: crossing 81/256,
nearest residual 97/256 versus baseline 85 and ordinary nearest-forward 99.
It also leaves the captured inverse at ...28fc rather than Excel ...28fd.
The control reproduces invoke on every input. No gamma inverse change lands.

Further rejected-model evidence: dnacalc-evidence/research/round58/20261112-fn2-rejected-0830/,
64 files, 120,226,186 bytes; checksum copy and one-way check zero differences.
These records do not add a landed pass or change any Excel answer.

## Fix2 bank controls: one-degree complement staging

Excel 20527, fresh serial service queries in `chi-half-fold-live.json`:
320 probabilities, including logarithmic tails and odd multiples of 2^-54.
CHIINV(p,1) preserves its exact answer after replacing p by
`1 - (0.5 + (0.5 - p))` on 320/320, versus 254/320 after `1 - (1-p)`.
The independent degree-two control preserves only 37/320 under the double
fold. This identifies probability staging specific to the half-shape arm;
it does not identify a public normal-inverse composition. The latter was
weak on `half-gamma-inverse-live.json` and is not used.

The retained change folds the df=1 probability through those two binary64
stores before the existing upper-gamma search. A surviving zero returns
#NUM!, as Excel does for tiny positive right probabilities. General degrees
retain their original right-tail surface. Replacing the whole right inverse
with GAMMA.INV(1-p,df/2,2) remains rejected: fresh paired controls are only
20/28 exact and broad bank checks lose existing exact words.

The search bracket now covers binary64's exponent range instead of stopping
after 200 doublings. This removes an artificial 2^200 ceiling in F inverses;
it does not make the remaining F inverse words exact. Probability endpoints
are admitted only after degree validation, including CHIINV(1,large_df)=0.

## Very large degrees are Goldstein's leading terms (fn9, 2026-11-17, dc-6332.18)

At n >= 2^53 the right-tail inverse (CHIINV, CHISQ.INV.RT) publishes x = n c^3 with
c = 1.0000886 + 0.4713941 z / sqrt(n), z the upper-tail normal deviate of p. The two constants are
the last two coefficients of R. B. Goldstein's chi-square quantile approximation (Comm. ACM 16(8),
1973, Algorithm 451); its other terms carry powers of 1/n and fall below the last place there.

- Found from the 11 structural rows (n = 8.9e10 to 3.4e22, ours #NUM!): a stored cube
  x = RN(n RN(RN(c c) c)) has a preimage c for 10 of 11 answers (shifted answers: 0 to 4 of 11).
  The c values read off are c - 1 = K + k z / sqrt(n) with K = 8.86000000000016e-5 and
  k = sqrt(2/9) * 0.999977893 = 0.4713941, residuals within 0.56 ulp of c.
- The measured large-shape routes this file's later sections and `large_shape_inverse_*` describe
  are the same formula: the unit factor 0x3ff00116bc774cb6 is RN(1.0000886^3), and their
  "neighbouring factors three binary64 words apart" are one-ulp steps of c in the cube.
- Bit check over every full-bank CHIINV and CHISQ.INV.RT row with n >= 2^53 and 0 < p < 1:
  244 of 244 (the 234 the measured routes matched plus 10 of the 11 structural rows), for each of
  six stagings of c (they cannot be told apart on these rows) and for z = NORMSINV(1 - p) or
  -NORMSINV(p). The n = 8.9e10 row is not a cube of this form: the 1/n terms show there.
- Landed for n in [2^53, 2^1000) and p in [0.0025, 1) (the measured range); GAMMA.INV's
  large-shape route (shared code) is unchanged.

### Down to n = 5e10: the 1/n terms (fn9, same day)

Below 2^53 the leading terms drift (12 ulp at 1e14, 1,375 at 1e12). Excel answers there are
still n c^3 with Goldstein's c, now with its 1/n terms. Read off at n = 1e11 (c minus the leading
terms, divided by f = 1/n, fitted as a polynomial in z): -0.223732 + 0.000135 z^2, Goldstein's
-0.2237368 and 1.348028e-4. Bit level: c is Horner in z with coefficients in f,
c = (1.0000886 - 0.2237368 f) + z (0.4713941 sqrt f + z 1.348028e-4 f), every product and sum
stored; the constant with its f term rounds first (sum orders without it miss by one ulp of c on
about a fifth of the points). n is the truncated degrees (the 8.9e10 bank row has a fraction).

- Excel returns #NUM! in a band from about 1e10 to 4.5e10 (at 3e10 for nearly every p; at 4e10 for
  z <= -0.31; at 1.2e10 for z >= -0.52): not modelled; the route starts at 5e10.
- Goldstein's higher terms (z^3 f^(3/2) and beyond) reach the last place at large |z| and small n:
  the route takes |z| <= 2.75 from 5e10 and |z| <= 4 from 2^40.
- Fresh, frozen before asking (chiq6: 5,000 points, |z| within the bounds, n from 5e10 to 2^90,
  non-integer degrees): CHISQ.INV.RT and CHIINV 4,990 of 5,000; the 10 misses are single-ulp
  cases at n <= 2.9e11. None of the published f^(3/2) coefficients placed on z f^(3/2) or
  z^3 f^(3/2) removes them (best 16,189 of 16,200 over all batches), so that residue is open.
- Raw answers: R2 dnacalc-evidence/research/round63/fn9-20261117/ (chiq1 to chiq6).
