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

## fn7 (2026-11-17): piece A identified and landed

Excel's ratio for `s = RN(t * t) <= 1/2` is

    R = RN(P(s) / Q(s)),  P, Q degree 7, Horner, every product and sum stored from x87 precision 64, Q(0) = 1

with constants (binary64 bits, low degree first)

    P: 3fec5bf891b4ef6b c00b03599ad6a33a 401492fc431b904d c00fce6e6f12a56b
       3ffa0cbd20e4ce55 bfd5444e88a51cee 3f9c034f2417c47b bf3c6105b261ae5d
    Q: 3ff0000000000000 c01049a6a9451cab 401ae761f165bea6 c01708d997e809b9
       40059bb87268973c bfe570c88781d342 3fb38664c121c0db bf65ae11dcadf003

How they were found (shape first, then constants):

1. **Denominator read-out.** In dense windows the answers are phase-coherent with stored P and Q on the 2^-53
   grid (the last Horner sum `1 + RN(s*B)` cancels): `Q(s_c) * E_i + D * sigma_i = 0 (mod 1)`, E the error of R
   in 2^-53 units, sigma the offset of s. Coherence 0.84 to 0.99 over 28 windows at s = 0.38 to 0.496 (random
   data: about 0.03). D = (P - Q)'(s) comes out to about 1e-5, Q(s) to about 0.5 % and about 2.5 % low at
   s = 0.5 (estimator bias, calibrated by running the same estimator on emulated answers). Script
   `fn7-dread2.py`. Both agree with an accurate (7,7) rational on an interval reaching past s = 0.66, so Q has no
   zero near 0.55 (round 49's linear 1/L fits were the wrong model, not wrong data).
2. **Fitting method.** With q1 = -4.0719248245643 and q2 = 6.72595946 (fn6), each candidate method has one free
   interval end b fixed by q1 and is tested by q2. Relative minimax: q2 off by +1.5e-3 (fn6). Maehly
   Chebyshev-Pade: -4.7e-5 to +4.8e-4. Rational interpolation at Chebyshev extrema or equispaced points:
   -2.6e-4, +1.9e-3. **Rational interpolation of erfinv(sqrt s)/sqrt s at the 15 zeros of T15 on [0, b], P(0)
   free, Q monic: q1 is met at b = 0.729999999999 and q2 then agrees to 1e-7.** Script `fn7-fam4.py`.
3. **Constants.** The exact interpolant on [0, 0.73], rounded, matches 436,842 of 656,542 retained answers (all of
   s < 1e-3, falling to 17 % near s = 0.5). Its function error is about half of Excel's at s = 0.5 (bias curve
   -4.1 against -7.7 units of 2^-53), and fewer typed digits or binary64 nodes or node values make it worse. The
   15 x 15 interpolation system has singular values from 4.8 down to 4.9e-17; a solve at limited precision moves
   the solution along the smallest singular vector. Scanning that one amplitude: at alpha = -2.0e-15 (alpha in
   [-2.05e-15, -1.93e-15]) the rounded constants match **656,542 of 656,542** retained answers. Two constant
   sets do so, differing in the last bit of p7. Scripts `fn7-nullscan.py`, `fn7-nullfine.py`, `fn7-emu.c`.
4. **Fresh proof.** Predictions were frozen before asking (`frozen.json`, sha256 f32781b1...): 20,000 random
   piece-A inputs, six dense 4,096-input windows at s = 0.46, 0.48, 0.495, 0.4995 (upper) and 0.47, 0.49 (lower),
   and 164 inputs where the two p7 candidates differ. Excel matches 44,576 of 44,576 window and random inputs,
   and picks p7 = bf3c6105b261ae5d on 164 of 164 (the other candidate 0 of 164).
5. **Dispatch.** An edge probe of 256 inputs around |t| = 1/sqrt 2 shows the test is `s <= 0.5` on the stored s
   (the lower input whose |t| equals binary64 1/sqrt 2 but whose s is 0.5 + 2^-53 is not piece A).

Through `calc_func`'s NORM.S.INV route: 685,936 of 685,936 retained and 44,870 of 44,870 fresh piece-A answers.
Raw answers: R2 `dnacalc-evidence/research/round63/fn7-20261117/` (13 files, 1,567,899 bytes).

Pieces B (1/sqrt 2 < |t| <= 0.85) and C (0.85 < t <= 0.875) are still the correctly rounded ratio. In piece B,
9,168 of 83,968 retained answers have no binary64 R with y = RN(t * R), so piece B does not form y as t times a
stored ratio; rational interpolants of erfinv or erfinv/t in t or s at Chebyshev zeros (intervals and degrees
scanned) reach at most 4 % there. Piece C's answers all have such a preimage.

## fn7: piece B (1/sqrt 2 < |t| <= 0.85), what is known and what failed

Data: the 13 retained R41 windows in B, plus 160,960 fresh Excel answers (20 windows of 2,048 at t = 0.7075 to 0.8495
and 120,000 random inputs, both sides); 244,928 distinct |t| after the odd fold (no conflicts). R2
`dnacalc-evidence/research/round63/fn7-20261117-B/` (4 files, 3,523,457 bytes). Scripts `fn7-B-*`.

- **Nodes.** The bias of y (100 bins, se 0.03 to 0.18 units of 2^-53) oscillates with 11 extrema between t = 0.710
  and 0.847. A fit of `c(t) + w(s) * (k0 + k1 t + k2 t^2)`, w the node polynomial of N Chebyshev zeros on [a, b],
  selects **N = 13 on s in [0.5, 0.73]** (chi^2 741 on 100 bins; [0.5, 0.7325] or [0.5, 0.7275]: 1,506 and 1,539;
  any N != 13 or variable t, -log(1-t), atanh t, sqrt(-log(1-t)): >= 2,331). Same upper end as piece A.
- **Final assembly.** No stored double R gives y = RN(t * R) for 31,259 of 244,928 answers (12.8 %), so piece A's
  `y = t * R` is not B's. Also refuted by unreachable answers: x = RN(RN(t sqrt2) * R) (23,779), x = RN(s * R)
  (21,299), y = RN(s * R) then x = RN(y sqrt2) (14,369). A final division (y or x = RN(N / D)) remains possible.
- **Constants.** Rational interpolants at those 13 nodes (m + n = 12, every split) of erfinv(sqrt s)/sqrt s,
  erfinv(sqrt s), and both times sqrt 2, in s or s - c (c = 0.5, 0.55, 0.6, 0.615, 0.62, 0.65, 0.73), with
  y = RN(P/Q), y = RN(t * P/Q) unstored, y = RN(RN(t P)/Q), x = RN(t * RN(P/Q)), x = RN(P/Q), each scanned along
  the system's smallest singular vector (alpha from 1e-20 to 1e-9, both signs): at most 57,318 of 244,928 bit
  matches (23 %), below the correctly rounded ratio. At the function level the same family misses the bias curve
  (best chi^2 3,525). So the node set is identified but not the function, variable or evaluation.
- The lattice read-out that found piece A's Q is swamped in B by the output grid (x > 1 there; peaks only at
  lambda = t, t/sqrt 2 and their combinations).

## fn9 (2026-11-17): piece B identified and landed; the tail's logarithm

The fable-r2 review (Claude Fable 5.2, `~/codex-runs/fable-r2/review.md`) found piece B's function and constants:

    v = 1 - |t|,  a = sqrt(-ln v),  s = RN(t * t),  R = RN(P(s) / Q(s)),  y = RN(a * R),  x = sign(t) * RN(y * RN(sqrt 2))

P degree 7 and Q degree 5 with Q(0) = 1, Horner in raw s with every product and sum stored from x87 precision 64;
P/Q is the exact rational interpolant of erfinv(sqrt s) / sqrt(-ln(1 - sqrt s)) at the 13 zeros of T13 on
[0.5, 0.73], rounded to binary64 (no perturbation). With binary64 `log` and `sqrt` it matched 244,847 of 244,928
retained answers; every other one needed the logarithm moved one ulp.

fn9 identified that logarithm: **a is FYL2X (ln 2 * log2 v) stored, then FSQRT stored**, the x87 staging Excel's
worksheet LN already uses (`excel_log`, `excel_sqrt_staged`). Retained piece-B answers 244,928 of 244,928 with it
(binary64 log and root 244,847; the runtime's FMA3 log 244,841; FYL2X with a binary64 root 244,868; binary64 log with
FSQRT 244,907). The AS241 tail forms `sqrt(-ln p)` the same way: the retained R41 lower-tail answers at normal inputs
match 393,216 of 393,216 (the runtime log with a binary64 root, the previous route, 393,140; FYL2X with a binary64 root
393,181). The 76 tail misses fn6 and fn7 attributed to "a detail of the logarithm" were this.

Dispatch: piece B is `|t| <= 0.85` (typed in t). On the lower side that is AS241's bound `q >= -0.425`; on the upper
side p = 0.925 itself (t = 0.85 + 1 ulp) is piece C in Excel (its retained answer is not B's), the double below is B.

Rust route over every retained and fresh answer on this host (1,369,488 rows: fn7's alldata plus fable-r2 q1-q3,
duplicates removed per region; before -> after): AS241 lower tail 393,140 -> 393,216 of 393,216; piece A 682,142 ->
682,142 of 682,142; piece B 29,928 -> 141,345 of 141,345; piece C 49,582 -> 49,582 of 94,207 (unchanged route).

### Piece C (0.85 < t, p <= 15/16): what is known (fn9)

- (a) refuted: piece B's program evaluated on C's inputs is 6,405 ulp of R off on average (2,888 of 94,208 inside the
  pinned interval).
- With a known exactly, each answer pins R to one double (38,794 of 94,208) or two (55,414). R is a function of t
  (or s, v), not of the stored a or the stored log: in the 8,192 inputs below p = 15/16, where the log's rounding is
  one constant, R's bias against erfinv(t)/a_true is smooth (+0.456 ulp; neighbours +0.463) and against
  erfinv/a_stored jumps (+0.162).
- R's bias is smooth, +0.36 ulp at t = 0.85 to +0.46 at 0.875. Rational interpolants at Chebyshev zeros with rounded
  coefficients in t, s, -ln v, a, 1/a, -ln(1-s), -1/ln v (raw or centred, 770 intervals, N 3..16, every split) fit
  that curve with chi^2 about 11 on 23 windows by the dozen, so the bias does not select a family.
- R stored beats R extended: st(a * RN(G + 0.4 ulp)) reproduces 64,902 of 94,208 answers, st(a * (G + 0.4 ulp))
  unstored 61,042.
- Noise fingerprint: the error spectrum in dense windows has sharp lines. One drifts at about -282 cycles per unit t
  up to the first noise step (t in (0.85464, 0.85472)) and vanishes there; another at about -390 appears after it; a
  third changes at the second step (t in (0.86921, 0.86928)); a weak line at 0.4385 -> 0.4456 cycles per input runs
  through all of C. A line that only changed binade would come back at twice its rate; these do not, so the steps
  look like changes of formula or of a dominant intermediate, not of one variable's binade.
- No emulated family (Horner of P/Q in the variables above, every product and sum stored; 1,589 families) reproduces
  the two steps: the best has a quarter of their size. Scripts and data `~/codex-runs/fn9/c/`.
