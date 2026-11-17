# NORM.S.INV central region: what is proven (fn6, 2026-11-17)

Bead dc-j9k.47. Continues DnaCalc-Research/design/normal-inverse.txt (rounds 42 to 49). Excel 16.0.20527 answers
asked through Touchstone; raw answers on R2 (prefix below). Scripts: `normal-inverse-central-2026-11-17/`.

## Landed

Between the AS241 tails (`p - 0.5 < -0.425`, `p > 15/16`) DNA Calc now publishes

    t = 2(p - 0.5)                 exact
    R = RN(erfinv(t) / t)          correctly rounded, from a double-double Newton inverse of the series erf
    y = x87_store(t * R)
    x = x87_store(y * RN(sqrt 2))

The Acklam start plus one binary64 Newton step it replaces lost up to 1,032 ULP near p = 1/2 (the residual
`(1 - p) - erfc/2` cancels there). The two stored products are Excel's (fix4 proved them near 1/2 with R = K).

## Proven about Excel's R (piece A, |t| <= 1/sqrt 2)

1. R is a stored double and is the quotient of two stored doubles: `R = RN(P(s) / Q(s))`, `s = RN(t * t)`, with
   P and Q Horner polynomials in s, `P(0) = K = RN(sqrt(pi)/2) = 0x3fec5bf891b4ef6b` and `Q(0) = 1` (monic).
   - At t < 1e-6 (1,309 fresh answers, log-uniform in t) the program `P = K + p1 s`, `Q = 1 + q1 s`,
     `p1 = c1 + K q1` (c1 the Taylor coefficient of erfinv(t)/t) matches 1,309 of 1,309 for q1 in
     [-4.0719248280, -4.0719248242]; q1 = 0 (a polynomial R) matches 1,060, `RN(erfinv/t)` about 970.
   - Adding `q2` and Taylor-matched `p2`, t < 1e-3: 1,504 of 1,506 for q1 = -4.0719248245644,
     q2 = 6.725955 (resolution about 1e-12 and 1e-5).
   - With q3 and Taylor-matched p3 (20,000 more answers, t log-uniform in (1e-3, 0.1)): t in [1e-3, 3e-3]
     5,602 of 5,638 and t in [3e-3, 5e-3] 1,979 of 2,000 for q1 = -4.0719248245643, q2 = 6.72595946,
     q3 = -5.756 (least squares on all answers: -5.7599). Across [1e-3, 1e-2] a joint scan reaches 2,922 of
     3,000 only with q4 = -0.2 (least squares 2.70), so from s ~ 1e-5 on q4 trades against q2 and q3 and the
     Taylor-matched numerator is no longer exact enough; this is where the progressive fit stopped.
   - Extended-precision (unstored) R, a stored `t*K + t*s*c1` sum, and w = -log((1-t)(1+t)) variables all
     fit worse (979, 872, 1,048 of 1,309).
2. The q1, q2 pair lies on the (7,7) family: relative minimax (7,7) rationals for erfinv(sqrt s)/sqrt s on
   [0, b] (rational Remez, mpmath) pass q1 = -4.0719248 at b = 0.6654 (P0 pinned) with q2 = 6.72742, 0.0015
   above Excel's; absolute, (1-s)-weighted and unpinned variants give the same +0.001 to +0.0016 offset.
   (6,8), (8,6), (7,6), (6,7), (7,8), (8,7), (5,8), (8,5) are further off (0.016 to 0.18 in q2), and AS241's
   own central rational expanded into powers of s has q1 = -4.15392, q2 = 7.01924 (it is the (7,7) minimax
   on [0, 0.7225]). Padé approximants about 0 give no q1 near -4.0719 for m + n < 30.
   So Excel's piece A is very likely a (7,7) near-minimax rational in s, monic denominator, fitted on an
   interval near [0, 2/3], whose higher coefficients are not the Remez values.
3. A linear least-squares fit of the remaining 12 coefficients to 100,000 Excel answers (q1, q2, P0 fixed)
   reproduces the function (residual 0.54 ulp rms) but not the bits (2,105 of 5,000 held-out answers, worse
   than the correctly rounded R): the coefficients are ill-conditioned and the answers depend on the rounding
   of P and Q, not on the function.
4. Band scans (dense answers in |t - t0| < 5e-5 t0, scanning the offset of Q(s) and the relative error of
   P/Q) see Excel's P/Q about +0.4 * 2^-53 above erfinv(t)/t for t in [1e-3, 1e-2], but the Q offset aliases
   (P moves with Q, so shifts of about 8 ulp nearly repeat): not yet a reliable coefficient readout.

## Next

Recover q3..q7 and p3..p7 progressively from bands at increasing t, scanning the Q offset jointly with its
slope over wider bands, then confirm on a frozen fresh set before changing the kernel. Pieces B
(1/sqrt 2 < |t| <= 0.85) and C (0.85 < t <= 0.875) are untouched here.

## Evidence

- Touchstone answers: `probe1.json` (6,000, t uniform in (0, 0.7071)), `probe2.json` (8,000, t log-uniform in
  (1e-7, 0.1)), `probe3.json` (100,000, t uniform in (0, 0.7071)), `probe4.json` (20,000, t log-uniform in (1e-3, 0.1)); R2 `dnacalc-evidence/research/round62/fn6-20261117/`.
- Scripts: `fx87.py` (exact x87 store emulation with integers), `step1.py`/`stepn.py` (coefficient scans),
  `remez.py`, `locus.py` (rational Remez and the (q1, q2) locus), `lsfix.py`, `score.py`, `band.py`.
