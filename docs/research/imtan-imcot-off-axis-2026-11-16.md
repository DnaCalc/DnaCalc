# IMTAN and IMCOT off the real axis: Excel readings and what they show (eng19, 2026-11-16)

Bead dc-vdma, cause C (10 jnt oracle cells). No kernel change landed: no operation graph reaches exact
agreement, so the cells stay failures. This note records what was measured so the next attempt starts
from it.

## Readings (Touchstone, Excel 16.0.20527, text arguments, answers are 15-digit text)

- 54 probes from `docs/plan/corpus-failures-2026-11-11.md` section C (large `y`, imaginary axis, tiny `y`).
- 1,200 random complex arguments per function (x from 1e-4 to 1e8, y from 1e-12 to 30, both signs).
- 800 imaginary-axis arguments `0±yi`, y log-uniform in 1e-9..25, and 300 dense steps of y in 1e-8..6e-8.
- The files are under `~/codex-runs/eng19/evidence/` on the Linux lane (not committed; Excel answers only).

## Findings

1. Large `y` needs no branch: `IMTAN("0+yi")` is `i` for every `y` from 20 up to 710 (tanh rounds to 1),
   `IMCOT` is `-i`; `y` of 354, 355, 700, 709, 709.78, 709.79, 710 and `1e100+355i`, `1e100+709.79i`
   all give `±i`. DNA Calc gives #NUM! there because `cosh(2y)` overflows. The legacy "branch on y" is
   just the saturated value.
2. IMCOT's denominator is `cosh 2y - cos 2x` with cancellation, and it is not the CRT `cosh`. On the
   imaginary axis `Im(IMCOT) = -sinh(2y) / den`, where `den` is always an even multiple of 2^-52 (the
   spacing of doubles at 1). With `c1 = (cosh y - 1) / 2^-52` exact, `den / 2^-52 = 2 * (RN(c1 - 1/8) +
   RN(c1 + 1/8))`, RN being round to nearest: 468 of 468 rows with y^2/2^-52 below 10^4, 611 of 642 over
   the whole set. The CRT form `2*cosh(y)^2 - 2` gives 565 of 742, `cosh(2y) - 1` 188 of 360. The
   -1/8 and +1/8 offsets are measured, not explained: no sum of the exp-based cosh pieces tried
   reproduces them. `IMCOT("0+1e-8i")` is #NUM! (den is 0 below y about 1.3e-8) and DNA Calc agrees.
3. IMTAN: on the imaginary axis `sinh(2y)/(1 + cosh(2y))` with CRT parts gives 750 of 800, an
   exp-based cosh with CRT sinh 755 of 800; no tried numerator/denominator form is exact.
4. Circular part: replacing the CRT `sin(2x)`, `cos(2x)` by the Excel sin chain moves IMTAN from 941 to
   1,066 of 1,254 (2·sin x·cos x with cos 2x as cos²x - sin²x), IMCOT from 880 to 1,056 of 1,254
   (2·sin x·cos x, cos 2x as 2cos²x - 1). The two functions use different cos 2x forms, as the earlier
   research found.

Open: the generating arithmetic of the hyperbolic factors (finding 2 and 3). Next probe that would settle
it: the same readings with `x` fixed at a nonzero value, to see whether the 1/8 offsets move with `x`.
