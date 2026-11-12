# Beta, F and t family: research log, 2026-11-12 (dc-6332.8)

Lane: Claude Fable 5.2 on dna-firehorse, headless. Excel answers: the Excel answer service (Excel
16.0 build 20527, env 29bedb2b6722), every numeric input sent as a binary64 bit pattern. Reference
values: mpmath at 200 to 400 bits. Our own arithmetic: calc-func's x87 chains (`excel_numeric`),
called from a scratch test harness that is not committed. The scripts and the cached Excel answers
of this run are on the host in `~/codex-runs/fable-r1/` (`cache/calls.log` has one line per
request; script names are given below as `x1.py`, `cfit3.py` and so on).

Sources: tier 1 only (Excel's answers, this repository's code and notes, DLMF identities). Two
published libraries are named below as shape analogies from general knowledge. Neither was read
in this session and no code was taken from them: Moshier's Cephes (`incbet` and its `pseries`;
this repository already names Cephes for the gamma fraction) and TOMS 708 (in the repository,
kept). Every constant, store and order below was fitted to Excel's bits, not taken from a source.

## What was identified

Excel's incomplete beta dispatches as TOMS 708's `bratio` does (the constants 0.7 and the
recurrence on the larger shape are confirmed below), but the bodies of its arms are not TOMS 708's.
They are simple programs built from pieces Excel shares with GAMMALN, BINOM.DIST and POISSON.DIST.

1. **Power series arm** (`bpser`; all rows of every test below). With `p = store(a * LN(x))`,
   `C = ln B(a, b)` as a binary64 value and `ai = store(1 / a)`:

   ```
   hi = store(p - C);  lo = store((p - hi) - C);  f = store(1 + lo)
   s = ai; t = 1
   for n = 1, 2, ...:
       u = store(store(store(n - b) * x) / n)
       t = store(t * u);  v = store(t / store(a + n));  s = store(s + v)
       stop when |v| <= 2^-53 * ai
   result = store(store(EXP(hi) * s) * f)
   ```

   Every store is a binary64 store of an x87 result at precision 64. The sum starts from `1/a`
   and takes the terms in order. The exponential is EXP of the stored difference, with what the
   store dropped put back as the last factor: the pattern POISSON.DIST's prefactor shows.
2. **`ln B(a, b)` with both shapes at most 8**: three public GAMMALN values, the larger shape
   first: `store(store(GAMMALN(max) - GAMMALN(a + b)) + GAMMALN(min))`. With a shape of 1 (only
   the recurrence arm asks) it is `-LN` of the other shape. Above 8 Excel uses a more accurate
   form, within about one unit of `ln B`; it is not a difference of rounded GAMMALN values and
   was not read.
3. **Beta density** (BETA.DIST with cumulative FALSE on the unit interval):
   - both shapes above 1: BINOM.DIST's mass of `a - 1` successes in `a + b - 2` trials (stored
     exponent and running normalizer), with one more running term `-LN(a + b - 1)`;
   - a shape of 1: the mass's end forms with the running term `-LN` of the other shape;
   - a shape below 1 (larger shape at most 8): `exp((a - 1) LN(x) + (b - 1) ln(1 - x) - ln B)`
     as running terms, the first product stored, the second by the internal logarithm.
4. **Recurrence arm** (`bup` for `b0 x0 > 0.7`): `x^a y^b / B(a, b)` is the stored `x y` times the
   beta density, with no scaling by `exp(mu)`; the terms and the final sum with the series are
   as in TOMS 708. One and two terms match every row on BETA.DIST; three or more match about 80%
   (the stores of the second and later terms are not read).
5. **BINOM.DIST cumulative** is the kernel's complement output at the pair `(p, q)`:
   `1 - I_p(k + 1, n - k)`, not `I_q(n - k, k + 1)` with `1 - q` formed again.
6. **FDIST / F.DIST / FINV**: with `n = store(d1 f)` and `r` the ratio of `n` and `d2` that is at
   least 1, the smaller beta argument is `1 / (1 + r)` and the larger `r / (1 + r)`, each stage
   stored; the kernel is called with the smaller argument first (`I(d2/2, d1/2)` when `n >= d2`,
   else `I(d1/2, d2/2)` and its complement output). With a first shape of 1 the F and t
   wrappers' kernel takes the stored `1 - x`. (Commit 1 had the pair `1 / (1 + n / d2)`,
   `1 / (1 + d2 / n)`, right only where the series runs; commit 2 has this form.)
7. **GAMMALN(2.5)** is one unit below what our table for `[2, 3]` gave; with Excel's value the
   recurrence gives GAMMALN(6.5) exactly as well. It is read at that one point (as 3.0 already was).

## Experiments, in order

Counts are Excel calls answered (cache misses); 264,002 in all: BETA.DIST 158,703, LN 43,877, FDIST 32,360, EXP 18,048, F.DIST 3,433, TDIST 2,400, BINOM.DIST 2,205, GAMMALN 1,739, T.DIST.2T 1,200, GAMMA 37.

### E0 Error scale (x0.py, 2,250 calls)
BETA.DIST(x, a, b), b in {2, 3, 2.5}, a from 3 to 300, 150 random x each. Excel's error against the
true value grows with the shape: mean 1.6 units at a = 3, 87 at a = 300, up to 807. So the
prefactor is an exponential of a stored logarithm-sized quantity, not a compensated product.

### E1 Which exponent is stored (x1b, x1c, x1e, x2: about 3,800 calls)
Method: if Excel is `exp(z) * F` with `z` one stored double and `F` accurate, then
`ln(Excel / F_true)` lies on the binary64 grid of `z`. For each candidate split the fraction of
rows within 0.05 of a grid point was counted (10% expected by chance).

- Prediction of "public GAMMALN sum in the exponent": the offset of
  `GAMMALN(a+b) - GAMMALN(a) - GAMMALN(b)` from `-ln B` at (300, 2) is 1,100 units; Excel's median
  error there is 41. Rejected for large shapes.
- Recurrence region (x in 0.3..0.99, a up to 300): no split shows a grid (9 to 21 of 120 rows).
  Rejected: one stored exponent times an accurate factor, also with the Loader mass exponents
  M(a, b), M(a, b-1), M(a-1, b), M(a-1, b-1).
- Series region (b x <= 0.7): the split `x^a` shows the grid on 150 of 150 rows at (40, 3):
  the stored product `a LN(x)` is in the answer, and `ln B` is not added to it in one store.

### E2 The tiny-x prefactor (x3, kmeas, cfit, cfit2, cfit3, cget: about 37,000 calls)
At `x < 2^-60` the series is 1. Frozen model: `Excel = exp(R(a LN(x))) * K(a, b)` with K a
shape-only constant. Verdict: holds, row spread +-1.5 units while raw errors are hundreds.
Exact form, 15 candidate programs scored on 8 shape pairs, about 37 rows each (cfit3.py):
`store(store(EXP(hi) * ai) * f)` is the only one at 100% on all eight; `store(EXP * f) / a` holds
only when `a` is a power of two, and the forms with `LN(a)` or `1/a` in the exponent hold about
half the rows.

### E3 The constant `C = ln B` (cget, cmap, cmap2, cord, creg, creg2, csmall: about 13,000 calls)
`C` was read as the one binary64 value that reproduces every row of 24 tiny-x calls per pair.

- Up to 8: equals `(GAMMALN(max) - GAMMALN(a+b)) + GAMMALN(min)` in binary64 at every pair where
  the orders differ (61 pairs listed by cord.py; e.g. (4, 6), (6, 7), (2.5, 4.5), (5.125, 5.5)),
  shapes down to 0.25. Its error against the true `ln B` reaches 6.9 units of `C`, as that
  difference of rounded values has.
- Above 8 (map on half-integers to 30, creg.py): `C` is within about one unit of the true value
  and equals the triple only by chance. The change sits between 8 and 8.25 in the larger shape:
  (2, 8) is the triple (3.0 units off the true value), (2, 8.25) is not.
- Rejected for all regions: one order for all pairs; the exact sum of the three values (one
  final rounding); LN of ratios of the public GAMMA (110 of 221 pairs, no better than the true
  value rounded).

### E4 The series (s1, s2, s3: about 6,400 calls)
Frozen candidates: TOMS's `c *= (0.5 + (0.5 - b/n)) x`, registers or stores, `bp (1 + a sum)`;
the Cephes order `(n - b) x / n` with the sum closed by the first term and `1/a`; and the sum
started from `1/a`. Verdict: only the last, all stored, matches every row: 120 of 120 at each of
(3, 3), (2, 4), (3, 1.5), (3, 2.5), (4, 7); b = 2 (one term) cannot separate them. The arm's
range (s3.py, 200 x per pair, 13 pairs): the program holds up to `b x = 0.7` and fails from the
next x on, at every pair: TOMS 708's dispatch constant.

### E5 The recurrence arm (b1..b19, u1..u13: about 20,000 calls)
- b = 2, a = 300 (b8..b16): the recurrence term is 99% of the answer. On consecutive doubles below
  0.5 Excel's answer is flat for two of every four x: it depends on x only through the stored
  `1 - x`. Its implied exponent is on the grid of `(a - 1) ln x` (87, 60, 76, 91 of 120 rows at
  four shapes; other splits 5 to 16): the term is `x y` times a density-like `x^(a-1)`.
- Frozen prediction: term = `store(store(x y) * BETA.DIST(y, 1, a, FALSE))` with Excel's own
  density, plus the series at `b = 1` with `C = -LN(a)`. Verdict: 879 of 880 calls at b = 2, eleven
  values of a from 2 to 20 (b19.py). With the triple for `C`, a = 7, 7.5 and 12 match no row.
- Rejected for the term: 18 register/store forms of `exp(a ln x + b ln y - C)` (at most 32 of 80).
- Two terms (b = 3, 2.5 with a to 6): 100 of 100 at four pairs with the repository's stores.
  Three or more terms: 64 to 85 of 100. Rejected there: 11 product orders times 4 sum orders in
  binary64, 50 register/store variants, Horner forms, per-term densities, and a shorter reduction
  (the full reduction to the fraction matches 82 rows, any other count at most 12).

### E6 The density (d1..d8, st1..st3: about 11,000 calls)
- (300, 1) and (1, 300): `exp(stored (a-1) ln x) * a` to 1.3 units; general shapes not.
- Integer shapes: within 1 to 3 units of `(a + b - 1) * BINOM.DIST(a - 1, a + b - 2, x, FALSE)`
  even at a = 300 where the mass itself is hundreds of units from the true value: the density is
  built on the mass. Of eight ways to bring in the factor, only the running term `-LN(a + b - 1)`
  after the normalizer matches: 99 or 100 of 100 at 11 pairs. With a shape of 1.5 it holds 63 to
  95 of 100: our Stirling error at 0.5 is one unit below Excel's (st3.py; 1.5 to 6.5 are exact).
- Unit shapes: 6 forms each side; `exp_running(internal log1p product, [-LN(b)])` and
  `exp_running(stored (a-1) LN(x), [-LN(a)])` match 100 of 100 at 9 shapes.
- A shape below 1: 32 forms; `exp_running(t1, [-t2, C])` with `t2` by the internal logarithm
  matches 100 of 100 at 12 pairs. The mass forms `a M(a, b-1) / x` and the like match under 31.
- No bank row is a density: this part rests on these calls alone.

### E7 BINOM.DIST (bn1.py, 1,500 calls)
n <= 9, random and round p. `I_q(n-k, k+1)` with `1 - q` formed again: 1,312; the complement
output at (p, q): 1,446. On the 175 rows where they differ Excel is the second on 150 and the
first on 16. Excel: BINOM.DIST(3, 7, 0.5, TRUE) = 0x3fdffffffffffffe, BETA.DIST(0.5, 4, 4, TRUE) =
0x3fe0000000000001.

### E8 The F wrapper (f1..f11: about 40,000 calls)
- With the series arm exact, FDIST is not Excel's own BETA.DIST at `d2 / (d2 + d1 f)`: on 112 series
  rows of (5, 6) only 77 agree, and the argument that would agree is up to 3 units away.
- Nine spellings of x scored through Excel's BETA.DIST: `1 / (1 + (d1 f) / d2)` matches 411 of
  411 rows at three pairs; the next best 304.
- Eight spellings of y scored on rows where the series runs on y: `1 / (1 + d2 / (d1 f))` matches
  1,200 of 1,200 at six pairs; `t / (1 + t)` 1,122, `1 - x` 827.
- With that pair our kernel matches all 951 fresh series rows of 17 degree pairs. The recurrence
  rows match about 60%: on an F pair (x + y is not 1 exactly) the recurrence term is not read
  yet; 25 combinations of product and logarithm argument were scored, none above 68%.
- Unit arm a = 1: FDIST(f, 7, 2) is `0.5 + (0.5 - exp(b LN(1 - x)))` on 199 of 200 calls and with
  the wrapper's y on 166.
- F.TEST (bank only, 78 rows): the earlier pair `d2 / (d2 + n)`, `n / (d2 + n)` holds 62, FDIST's
  new pair 39, forms on the variances or sums of squares 33 to 44. F.TEST keeps the earlier pair.
- TDIST: two tails equal Excel's BETA.DIST at `v / (v + t^2)` on 854 of 855 rows where the
  series runs on x; unchanged.

### E8b The F pair where both arguments are used (f12..f25: about 30,000 calls, after commit 1)
- With the commit-1 pair the recurrence rows matched about 68%. Rejected one at a time, each
  with Excel's own BETA.DIST density and LN/EXP (so no error of ours enters): 5 spellings of the
  density's argument times 5 products (best 68%), 11 products, 6 spellings of the series
  argument, a double complement, and the series arm on its own (5 to 93 of 256..414 rows).
- The decisive control: on 1,043 calls where the two arguments sum to one exactly and x >= 0.5
  (nothing left to choose in the pair), FDIST equals Excel's own BETA.DIST(x, a, b) on 453 and
  its complement form on 540, with differences up to 8 units. So the wrapper's pair was wrong
  there, not the kernel's use of it.
- 36 pairs of spellings scored on rows where neither orientation's series applies: `1/(1+t)`
  with `t/(1+t)`, `t = n/d2`, matches 1,679 of 1,936; the commit-1 pair 1,470. Split by `t`
  (f24.py, 10 degree pairs, about 2,900 mid rows): below `t = 1` only the pair
  `(t'/(1+t'), 1/(1+t'))`, `t' = d2/n`, matches, above it only `(1/(1+t), t/(1+t))`; no row
  matches neither at the pairs whose arms are exact.
- Frozen prediction, then fresh calls (f7.py, 17 degree pairs, 150 f each): series 951 of 951;
  recurrence with one term 190 of 190 and 15 of 15 before the asymptotic arm; two terms 301 of
  303; three to five terms 381 of 491; a shape of 1 or 1/2: 551 of 600.
- Orientation in the unit arms (f25.py): at `d1 = 2` and `n < d2` Excel's answer is that of the
  kernel called as `I_small(1, d2/2)` (154 of 200), not `I_large(d2/2, 1)` (144).
- F.TEST (bank only): this form holds 48 of 78 rows, its earlier pair 62, eight forms on the
  variances or sums of squares 39 to 45. Unchanged.

### E5b Three or more recurrence terms (u14..u21: about 6,000 calls)
- On dyadic x (k/16) all 31 calls at b = 4 match every plain form: the structure is right and
  the misses are rounding order.
- The implied sum `w` (from Excel's answer, our series and Excel's density) sits a little off
  the exact sum by a shape-only amount; fitted as a relative error of the second term it agrees
  in sign and roughly in size with the binary64 rounding of the quotient `(a+b+l)/(a+1+l)` (zero
  at (7, 4) and (6.25, 4), where the quotients are exact): the quotient is stored.
- Rejected: stores rounded toward zero (as GAMMA.DIST's series has); 128 register/store
  combinations in a precision-64 emulation, best 514 of 600 (quotient stored, the term kept in
  the register for the next step, its stored value added to a stored sum, product stored).
  Not landed: no form reaches all rows.

### E5c Three or more terms on few-bit arguments (u22..u37: about 9,000 calls; nothing landed)
A sharper control than E5b. With `1 - x = k / 2048` and integer shapes whose quotients are exact
((7, 4): 8/2 and 9/3), every term and the sum `w = 1 + 4y + 12y^2` are exact in binary64, so no
register or store order can matter. Frozen prediction: all rows match. Verdict: BETA.DIST(x, 7, 3)
matches 880 of 880 and (6, 3) 880 of 880, but (7, 4) misses 35 of 937, all of them with x in
[0.39, 0.49) (y from 0.5 until w reaches 8), one unit either way; outside that band none. (6, 4)
has the same band and others, (5, 5) a band for x in [0.18, 0.25) (until w reaches 16).
- In the band the series part and the first factor are right (the b - 1 call matches on all 201
  rows there), and Excel's answer needs `w` one unit off its exact value.
- Rejected on the band: 14 assemblies of the final sum, 11 orders of the term with and without a
  stored reciprocal (the reciprocal forms do raise (5, 5) from 665 to 707 of 716), a shorter
  reduction (at most 12 of 201), sums rounded toward zero.
- Reading: Excel's terms carry a rounding error of about a unit of each term even when the term
  is exactly representable, which shows only where a term's unit reaches half the sum's (the
  band's edges fit that: the first term passes 2 at its start and the sum passes 8 at its end).
  So the terms are not formed by the products tried here; what does form them is the open
  question for this arm (427 FDIST and about 130 BETA.DIST bank rows).

### E10 The asymptotic arm for a large and b <= 1 (g1b..g6b: about 6,000 calls; nothing landed)
- 829 of the 1,186 BETA.DIST bank rows still failing are here (x > 0.5, a > 15, b <= 1).
- Excel's error against the true value grows with a (one sigma 5.5 units at a = 20, 34 at 200).
  Carried through `z = store(nu * LN(x))`, `nu = a + (b - 1)/2` in binary64, it falls to 4 units
  at a = 200 for b in {0.125, 0.2, 0.25, 0.3, 1/3, 0.4, 0.6, 0.7, 0.9}; one unit either side of
  that `nu` leaves 26. So this arm is TOMS 708's `bgrat` in shape, with `z` from the public LN at
  every x (TOMS switches to a rational for `ln(1 - y)` below y = 0.375; with LN the 90th
  percentile of our distance at (200, 0.3) falls from 59 units to 5).
- b = 0.5 exactly does not fit (24 of 34 units left): another path there.
- Not landed: the bank moves by one row and exact matches stay near 8%; the pieces after `z`
  (the gamma prefactor, the gamma tail, the ratio and the correction sum) are not read.

### E9 GAMMALN (g1.py, g2: about 1,300 calls)
Our GAMMALN against Excel's: 200 of 200 on 8..20, 183 on 3..8, 73 on 2..3, 50 on 1.5..2, 66 on
0.7..1.5. Every `ln B` of shapes off the half-integers inherits this. At 2.5 ours was one unit
high; with Excel's value 6.5 follows.

## Bank counts (`cargo xtask witness judge --bank full`, commit 1)

All functions: 3,889,526 -> 3,892,751 of 3,910,773 rows (+3,225). No function outside this table
moved.

| Function | before | after | gained | lost |
|---|---|---|---|---|
| FDIST | 661 | 2,988 of 4,399 | 2,499 | 172 |
| BETA.DIST | 1,043 | 1,636 of 2,822 | 629 | 36 |
| F.DIST | 180 | 282 of 369 | 133 | 31 |
| BETADIST | 143 | 195 of 294 | 58 | 6 |
| NEGBINOM.DIST | 450 | 497 of 510 | 47 | 0 |
| F.TEST | 26 | 62 of 78 | 38 | 2 |
| TDIST | 28 | 56 of 92 | 28 | 0 |
| BETAINV | 18 | 25 of 62 | 10 | 3 |
| BINOM.DIST | 70,320 | 70,326 of 70,349 | 6 | 0 |
| BINOM.INV | 2,302 | 2,308 of 2,308 | 6 | 0 |
| CRITBINOM | 1,414 | 1,420 of 1,420 | 6 | 0 |
| T.DIST.RT, F.DIST.RT | 1, 5 | 4, 6 of 6 | 4 | 0 |
| FINV, TINV | 3, 22 | 4, 22 | 4 | 3 |
| GAMMA.DIST | 77,815 | 77,821 of 77,975 | 6 | 0 |
| GAMMALN, GAMMALN.PRECISE | 2,137, 10,645 | 2,139, 10,647 | 4 | 0 |

The 253 rows lost are listed in `beta-family-2026-11-12-lost-rows.tsv` (function, id, source,
arguments, Excel, calculated, units off). By cause: the recurrence term at an F argument pair or
with three or more terms (FDIST 172, F.DIST 31, and BETA.DIST rows with integer or half-integer
shapes); `ln B` of shapes off the half-integers, where GAMMALN below 3 is not exact (BETA.DIST,
BETADIST); the inverses following a changed forward (BETAINV 3, FINV 2, TINV 1); F.TEST 2. The
tests that pinned three of them (`f1_stats.rs`: FDIST at 5 and 6 degrees on two adjacent values,
F.DIST(1.324..., 4, 11)) now record the calculation beside Excel's answer.

## Bank counts, commit 2 (the F tails)

All functions: 3,892,751 -> 3,893,685 (+934). FDIST 2,988 -> 3,861 of 4,399 (884 gained, 11
lost); F.DIST 282 -> 343 of 369 (61 gained, none lost). No other function moved. The 11 rows
lost are in `beta-family-2026-11-12-lost-rows-2.tsv`: all are recurrence rows with three or more
terms (degree pairs (9, 3), (5, 11), (8, 9), (9, 11), (7, 7)) that matched by one unit of luck
under the earlier pair. The three `f1_stats.rs` rows recorded as lost at commit 1 match again
and are exact assertions again.

## Open, in the order I would take them

1. The stores of the recurrence's second and later terms (most of the 538 FDIST rows left, and
   about 150 BETA.DIST rows).
2. The asymptotic arm for a large and b <= 1 (829 BETA.DIST rows): `z` is read (E10), its other
   pieces are not; then `ln B` above 8, `bfrac`, `basym`, and the `min <= 1` recurrence arm
   (`bup_140`), where Excel's density form lost 3 TDIST rows and was left out.
3. GAMMALN below 3 (2,139 of 4,538): every shape off the half-integers depends on it.
4. The Stirling error at 0.5 (one unit), for densities with a shape of 1.5.
5. The inverses (BETAINV, FINV, TINV follow the forward; their own search is not read) and
   F.TEST's argument pair and tie rule.
