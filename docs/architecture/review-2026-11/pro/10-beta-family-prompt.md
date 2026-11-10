# Question for GPT-6 Pro: how does Excel compute the incomplete-beta family (BETA.DIST, BETADIST, BETAINV, FDIST, F.DIST, FINV, F.TEST, TDIST, TINV, NEGBINOM.DIST, BINOM.DIST, HYPGEOM.DIST), and where does our TOMS 708 port part from it?

DNA Calc is a Rust re-implementation of an Excel-compatible spreadsheet that must reproduce Excel's numbers
bit for bit. The repository is public: https://github.com/DnaCalc/DnaCalc (MIT). This question is pinned to
commit `b2a808502f74e1590dcfc682171c8a1db1d93ee2`
(https://github.com/DnaCalc/DnaCalc/tree/b2a808502f74e1590dcfc682171c8a1db1d93ee2). Every `file:line` below
refers to that commit (master HEAD b2a80850, which already contains two fixes that came out of an earlier version of this question, listed in Appendix C) and is relative to the repository root. You may read the repository if you can;
everything you need is in the appendices, so do not depend on browsing.

Excel's answers come from our Excel answer service on the oracle machine (AMD Ryzen 7 4800H, Windows x64; Excel
builds 20131 for most of the beta, F and t rows, 20228 for part of the F rows, 20430 and 20527 for the discrete ones). Everything numeric below is binary64. "ulp" means the distance in representable
doubles between our answer and Excel's; the sign is "ours minus Excel's" in the lattice, so a positive number
means our value is the larger one. "Exact" means all 64 bits equal.

The situation in one paragraph. Excel's incomplete-beta family is the largest block of numeric functions we cannot
match (the whole bank stands at 3,889,129 of 3,910,795): BETA.DIST 1,003 of 2,822 rows exact, BETADIST 142/294, BETAINV 18/62, FDIST 661/4,399, F.DIST 180/369, FINV 3/32,
F.TEST 26/78, TDIST 28/92, TINV 15/32, NEGBINOM.DIST 450/510, HYPGEOM.DIST 3,572/3,661, BINOM.DIST 70,320/70,349 (the
last three counts are flattered by rows that are error values or by cases that never reach the beta; Appendix A.1).
Our kernel is the ACM TOMS 708 `bratio` (kept in our tree under the licence note in our source rules) with unit-shape
arms we identified separately. Misses are mostly a few ulp but reach 170 ulp: Excel's *own* noise is larger and
differently shaped than a careful port's (series arms with a large exponent). Three rounds of research (R33, R43-R45) did not
find the shape. We want it.

**Outstanding, in priority order** (two earlier findings are already landed, see Appendix C): (1) the F-to-beta argument: FDIST's answers
resolve x finer than any binary64 pair (v, w) can (A.6); (2) the beta prefactor and arm structure, with the Loader-type lead from the exactly matching gamma, Poisson and binomial masses (A.3, A.12);
(3) BETA.DIST's remaining blocks: a,b > 1 below the mean (12% exact), the large-a small-b block (8% exact although Excel's and our error scales are now equal), tiny x; (4) the inverses: Excel's TINV is up to 176 ulp from the true root and
BETAINV/FINV a few ulp off it, so Excel does not converge to the lattice (A.4); (5) TDIST (28/92, A.7); (6) NEGBINOM.DIST cumulative, BINOM.DIST's beta route, HYPGEOM at larger N (A.9-A.11).

## What we ask

Please produce, in this order:

**A. For each function, the most plausible computation shape Excel uses**, with the evidence for and against from the
data in the appendices, separating what the data show from what you infer. At least:
1. BETA.DIST / BETADIST (cumulative; BETADIST(x,a,b) is the same computation, Appendix A.1): the series versus
   continued-fraction choice and the exact switch rule; the form of the prefactor x^a (1-x)^b / (a B(a,b)) and how
   each factor is computed (what logarithm or Stirling-type correction, how the beta function is formed, what is
   stored in binary64 and what is not); the rule for the symmetry swap x <-> 1-x (a <-> b); how the complement is formed;
   what the asymptotic region (a large, b <= 1) does; whether TOMS 708's arm structure is present at all (our landed change, bgrat at eps instead of 15 eps, made the asymptotic block unbiased but it is still only 8% exact; the 8 rows it lost are a lead, Appendix C).
2. FDIST, F.DIST, F.TEST: how F maps to the beta (which argument pair, how formed, which tail is computed first),
   given the argument-resolution finding in A.6. For F.TEST: the 2*min(p, q) rule is landed (26/78 exact); what remains is the last-bit structure and the variance computation.
3. TDIST, TINV: the mapping of t to the beta (and the df = 1, 2 closed forms), and why tails = 1 is exactly half of tails = 2.
4. BETAINV, FINV, TINV: the inverse's iteration (method, start, stopping rule); the data say Excel's result for TINV is
   sometimes 176 ulp from the true root while our bit-converged bisection of our own forward is within 3.
5. NEGBINOM.DIST, BINOM.DIST, HYPGEOM.DIST: whether and how their cumulative forms use the incomplete beta, and what
   the exact-matching mass functions tell about the beta prefactor (Appendix A.12: the gamma/Poisson/binomial masses
   match to the last bit with a Loader-type saddle-point prefactor, the beta does not).
6. The beta density (BETA.DIST with cumulative = FALSE): no row of it is in our bank, so say what Excel most likely does
   and give the experiments (Appendix B shows our unmeasured guess).

**B. Ranked alternatives with discriminating experiments.** For each hypothesis you rank, give specific Excel calls
(function, arguments as hexadecimal doubles, or as a rule for generating them, such as "the 257 consecutive doubles
from this one") and the outcome predicted under each hypothesis, so that one request to our answer service separates
them. We can ask thousands of calls per request in seconds, adjacent-double windows, dyadic inputs, extreme tails,
and any argument Excel accepts. Prefer experiments whose outcome is a yes/no on a structural question.

**C. An implementable specification of the most likely shape**: formulas, evaluation order, which intermediates are
rounded to binary64 and which are kept wider, constants and thresholds as numbers, for every function in A. A Sonnet
agent will implement it in Rust and run it against the 82,700 recorded Excel answers behind this question (70,349 of them BINOM.DIST masses). Where you
cannot give a constant, say which experiment would produce it.

**D. Bold ideas**: anything that explains several of the observations at once, including ideas you are not sure of.

## Constraints that bind the answer

1. Bit-exact parity with Excel is the goal. The answer is judged by recorded Excel answers.
2. Source rules (docs/plan/RESEARCH_SOURCES.md of our repository): tier 1, free to read: Abramowitz and Stegun,
   DLMF, Hart et al., Cody and other published papers, Intel SDM, IEEE 754, textbook mathematics, and Excel's own
   outputs. Tier 2, permissive code: Boost.Math (BSL-1.0), fdlibm/msun/openlibm, Arm optimized-routines, AMD libm,
   CORE-MATH, SciPy cdflib (public domain), SLATEC/FNLIB, Cody's SPECFUN. Tier 3, facts only: Numerical Recipes, R
   nmath, glibc libm, ACM TOMS code other than what we already keep. Tier 4, never used: Microsoft source,
   disassembly or decompilation of Excel or of Microsoft runtime DLLs. You may use tiers 1 and 2 freely. For tier 3
   give facts only (method family, published origin, constants as numbers), never code, structure or names.
   Never use tier 4. Black-box reasoning from Excel's outputs is welcome. (We treat Loader's saddle-point binomial paper, Stirling error and deviance, A&S, the Didonato-Morris paper on which TOMS 708 rests,
   and Lentz's and Temme's methods as published papers, tier 1.)
3. Method: the "code shape" hypothesis. Excel's numeric kernels are mostly simple hand-written 1990s C: double
   arithmetic, perhaps x87-era habits (extended intermediates), the Microsoft C runtime's libm. Rank candidate
   computations by what someone then would plausibly have typed, find the shape first, then fit constants. A match
   percentage alone is not evidence of the right shape; the TOMS 708 shape got "29-60% exact per branch" and that
   did not make it right.
4. You cannot run anything. Reason from the data given. Where a claim depends on arithmetic you cannot do exactly
   (rounding at the last bit), say so and design the experiment instead.
5. Every proposed change must be implementable: the exact computation in prose or pseudocode.
6. Excel's oddities are to be copied bit for bit; no "improved" accuracy.

## How to read the appendices

- Appendix A: measurements (counts, regions, ulp distributions, signs, where matching starts and stops) and our
  reading of them. A.12 lists the functions that already match, and how.
- Appendix B: our code, with `file:line`.
- Appendix C: what earlier research rounds concluded (round numbers R33, R43, R44, R45, ...), including where they
  disagree with each other or with this reading.
- Branch labels used in the tables name the arm of our `bratio` that handles the row (Appendix B shows the dispatch):
  `bpser` power series, `bup` the recurrence on a (I_x(a,b) - I_x(a+n,b)), `bfrac` Lentz-type continued fraction,
  `bgrat` Temme-type asymptotic expansion for large a, small b, `basym` large-a,b expansion, "swap" = the arm ran on
  (b, a, 1-x, x) and the answer is the complement. `S:` = min(a,b) <= 1, `L:` = both shapes > 1.
- Inputs in the row listings are shortest round-trip decimals, which identify each double exactly; results are given as 64-bit hex and as 17-digit decimals.
- "true" means the exact value (computed offline with 50-digit arithmetic on the exact binary64 inputs, after forming z = (x-A)/(B-A) in binary64 for BETA.DIST with limits). "err vs true" is
  (value - true) / ulp(value), so it measures how far Excel and our code each are from the exact answer.

---

# Appendix A: measurements

## A.1 The bank and what its counts hide

- Source: our witness bank (Excel answers recorded over many sessions), the `full` bank rows. Row counts and
  matches, at HEAD b2a80850: BETA.DIST 1,003/2,822; BETADIST 142/294; BETAINV 18/62; FDIST 661/4,399; F.DIST 180/369; FINV 3/32;
  F.TEST 26/78; TDIST 28/92; TINV 15/32; NEGBINOM.DIST 450/510; HYPGEOM.DIST 3,572/3,661; BINOM.DIST 70,320/70,349.
- **BETA.DIST rows are all cumulative = TRUE.** There is no density row for BETA.DIST (or F.DIST, T.DIST) in the bank.
  600 BETA.DIST rows carry limits A, B (z = (x-A)/(B-A) is formed in binary64; in 486 of them x itself lies outside [0,1]);
  no row has z = 0 or z = 1.
- **BETADIST(x,a,b) rows are the same 294 inputs as BETA.DIST(x,a,b,TRUE), and Excel's answers are identical on all 294.**
- **F.DIST rows are all cumulative = TRUE** (369); FDIST is the right tail.
- **TDIST: tails = 1 equals tails = 2 divided by 2 exactly on all 46 paired inputs** (our code does 0.5 * the same value).
- NEGBINOM.DIST: 240 of the 510 rows are #NUM! (agreeing), leaving 270 numeric rows (129 mass, all exact; 141
  cumulative, 81 exact). HYPGEOM.DIST: 2,122 of 3,661 are #NUM! rows (agreeing); 1,539 numeric rows, 1,450 exact, with
  population N at most 48. BINOM.DIST: 70,335 of the rows are masses (cumulative = FALSE) or k = 0 / k = n; only 14 rows
  are interior cumulative values (0 < k < n) that reach the beta.
- Excel's own accuracy: for BETA.DIST the median absolute error vs the true value is 1.4 ulp (p90 7.5, max 116);
  ours 1.4 (p90 6.9, max 134; it was 1.7, p90 28.5 before the bgrat change). For FDIST Excel 1.5 (p90 5.4, max 21), ours 1.9. For F.DIST Excel 0.5 (p90 1.7, max 5).
  So **Excel is itself several ulp off the true value, with the error growing with the size of the exponent**
  (A.3), and a "more accurate" implementation does not match it.

## A.2 Ulp distribution and error sign, per function (numeric rows; "ours>Excel" counts rows where our value is the larger)

| function (numeric rows) | 0 | 1 | 2 | 3-4 | 5-8 | 9-16 | 17-32 | 33-64 | 65+ | ours>Excel | ours<Excel |
|---|---|---|---|---|---|---|---|---|---|---|---|
| BETA.DIST (2822) | 1003 | 429 | 317 | 367 | 324 | 274 | 78 | 23 | 7 | 1000 | 819 |
| BETADIST (294) | 142 | 39 | 39 | 27 | 18 | 14 | 10 | 2 | 3 | 83 | 69 |
| BETAINV (62) | 18 | 28 | 7 | 5 | 3 | 1 | 0 | 0 | 0 | 27 | 17 |
| FDIST (4399) | 661 | 1028 | 847 | 902 | 593 | 355 | 13 | 0 | 0 | 1784 | 1954 |
| F.DIST (369) | 180 | 95 | 74 | 16 | 4 | 0 | 0 | 0 | 0 | 94 | 95 |
| FINV (32) | 3 | 7 | 6 | 7 | 4 | 2 | 2 | 1 | 0 | 11 | 18 |
| TDIST (92) | 28 | 14 | 10 | 14 | 18 | 6 | 2 | 0 | 0 | 26 | 38 |
| TINV (32) | 15 | 3 | 3 | 0 | 4 | 3 | 2 | 1 | 1 | 7 | 10 |
| NEGBINOM.DIST (270) | 210 | 28 | 5 | 6 | 4 | 5 | 5 | 1 | 6 | 30 | 30 |
| HYPGEOM.DIST (1539) | 1450 | 34 | 33 | 10 | 9 | 1 | 2 | 0 | 0 | 54 | 35 |
| BINOM.DIST (70349) | 70320 | 10 | 3 | 0 | 3 | 5 | 1 | 0 | 7 | 7 | 22 |

Reading: the sign of the difference is balanced for every function including BETA.DIST (ours higher on 1,000 rows, lower on 819; before the bgrat change 725 against 1,115, almost all from one arm, A.3) and BINOM.DIST (the 7 large cumulative misses are all ours lower,
in the extreme lower tail, results between 1e-150 and 1e-130).

## A.3 BETA.DIST by region (2,822 rows)

By the arm of our `bratio` that handles the row (a row appears in exactly one line). "Excel abs err" is the mean over
the rows of |Excel - true| in ulp, "signed" the mean of (Excel - true).

| branch our code takes | rows | exact | median ulp of misses | max ulp | ours>Excel / ours<Excel | Excel mean abs error vs true (mean signed), ulp | ours mean abs error vs true (signed), ulp |
|---|---|---|---|---|---|---|---|
| S:bgrat swap -- min(a,b)<=1, x>0.5 so swapped; then bgrat (in practice a>15, b<=1) | 943 | 75 (8%) | 4 | 146 | 511 / 357 | 4.7 (-0.1) | 4.6 (+1.3) |
| L:bup_bpser swap -- same, after swap | 371 | 235 (63%) | 1 | 8 | 67 / 69 | 0.6 (-0.0) | 0.8 (-0.1) |
| L:bup_bpser -- a,b>1, b0<40 and b0*x0>0.7: bup + bpser, no swap | 342 | 43 (13%) | 3 | 170 | 188 / 111 | 4.0 (+0.4) | 3.8 (+1.2) |
| L:bpser swap -- a,b>1, x above the mean (swap), power series | 259 | 231 (89%) | 1 | 10 | 15 / 13 | 0.4 (+0.1) | 0.4 (+0.0) |
| L:bpser -- a,b>1, x below the mean (lambda>=0), power series | 247 | 30 (12%) | 4 | 67 | 88 / 129 | 8.6 (+3.4) | 5.3 (+0.2) |
| S:bpser swap -- min(a,b)<=1, bpser after the x<->1-x swap | 153 | 117 (76%) | 1 | 4 | 24 / 12 | 0.5 (+0.2) | 0.5 (+0.3) |
| S:bpser -- min(a,b)<=1, bpser(a,b,x), no swap | 144 | 15 (10%) | 3 | 43 | 56 / 73 | 3.4 (-0.0) | 3.8 (+0.3) |
| b1_exp -- b = 1, x <= 0.9: exp(a ln x) arm | 111 | 111 (100%) | - | - | 0 / 0 | 1.1 (+0.2) | 1.1 (+0.2) |
| a1_hi -- a = 1, x >= 0.1: complement arm | 89 | 89 (100%) | - | - | 0 / 0 | 0.3 (-0.1) | 0.3 (-0.1) |
| S:bpser_sym -- min(a,b)<=1, bpser on the complement, no swap | 29 | 15 (52%) | 2 | 7 | 14 / 0 | 0.5 (+0.1) | 1.5 (+1.4) |
| S:bpser_sym swap -- same, after swap | 29 | 3 (10%) | 5 | 23 | 1 / 25 | 2.8 (-1.5) | 6.9 (-6.9) |
| S:bup20+bgrat swap -- same, after swap | 28 | 6 (21%) | 2 | 38 | 13 / 9 | 3.8 (-0.6) | 2.5 (+0.5) |
| S:bup20+bgrat -- min(a,b)<=1, bup(20) then bgrat, no swap | 26 | 6 (23%) | 2 | 12 | 5 / 15 | 2.6 (+0.9) | 1.8 (-0.8) |
| L:bup_bgrat -- a,b>1, b0<40, x0>0.7: bup + bgrat, no swap | 21 | 4 (19%) | 4 | 14 | 14 / 3 | 2.8 (+0.3) | 3.1 (+2.6) |
| a1_lo_expm1 -- a = 1, x < 0.1: -expm1(b log1p(-x)) arm (landed R43) | 12 | 12 (100%) | - | - | 0 / 0 | 0.3 (+0.2) | 0.3 (+0.2) |
| L:bup_bgrat swap -- same, after swap | 9 | 7 (78%) | 5 | 5 | 1 / 1 | 0.6 (+0.4) | 0.6 (+0.1) |
| S:bgrat -- min(a,b)<=1, bgrat, no swap | 5 | 4 (80%) | 2 | 2 | 0 / 1 | 0.8 (+0.1) | 0.4 (-0.3) |
| L:bfrac -- a,b>1, b0>=40: continued fraction | 4 | 0 (0%) | 18 | 76 | 3 / 1 | 11.6 (-6.0) | 16.9 (+11.5) |

Where matching starts and stops, by the size of the answer (rows grouped by -ln of Excel's result):

| -ln(Excel result) | rows | exact | median ulp of misses | max | ours>Excel / ours<Excel | Excel abs err vs true (signed) | ours abs err vs true (signed) |
|---|---|---|---|---|---|---|---|
| [0, 0.1) | 542 | 507 (94%) | 1 | 2 | 21 / 14 | 0.3 (-0.0) | 0.3 (+0.0) |
| [0.1, 0.5) | 310 | 168 (54%) | 1 | 12 | 75 / 67 | 0.8 (+0.1) | 0.9 (+0.1) |
| [0.5, 1) | 288 | 85 (30%) | 2 | 17 | 105 / 98 | 2.0 (+0.1) | 2.2 (-0.3) |
| [1, 2) | 374 | 81 (22%) | 2 | 12 | 172 / 121 | 2.0 (-0.8) | 2.2 (-0.4) |
| [2, 4) | 655 | 96 (15%) | 4 | 38 | 307 / 252 | 3.6 (-0.2) | 3.1 (-0.3) |
| [4, 8) | 414 | 40 (10%) | 6 | 33 | 201 / 173 | 5.4 (+0.4) | 3.7 (+0.9) |
| [8, 16) | 147 | 19 (13%) | 8 | 126 | 61 / 67 | 8.8 (+4.0) | 7.0 (+1.6) |
| [16, 32) | 54 | 6 (11%) | 11 | 90 | 35 / 13 | 10.2 (-2.4) | 15.0 (+8.1) |
| [32, 64) | 26 | 1 (4%) | 14 | 170 | 11 / 14 | 32.8 (+21.8) | 27.2 (+21.0) |
| [64, 1000) | 12 | 0 (0%) | 36 | 146 | 12 / 0 | 28.1 (+5.5) | 51.6 (+48.0) |

For a, b > 1 by position relative to the mean a/(a+b) (the swap region is x above the mean):

| x / mean, a,b>1 (mean = a/(a+b)) | rows | exact | med ulp | max | >/< | Excel abs err | ours abs err |
|---|---|---|---|---|---|---|---|
| [0, 0.25) | 154 | 16 (10%) | 7 | 170 | 47 / 91 | 12.5 (+4.8) | 8.2 (-0.3) |
| [0.25, 0.5) | 146 | 11 (8%) | 4 | 33 | 73 / 62 | 6.4 (+1.1) | 3.7 (+1.3) |
| [0.5, 0.75) | 153 | 21 (14%) | 4 | 26 | 83 / 49 | 3.0 (+0.3) | 3.6 (+1.5) |
| [0.75, 0.9) | 92 | 14 (15%) | 2 | 14 | 55 / 23 | 1.8 (-0.0) | 2.6 (+1.6) |
| [0.9, 1) | 66 | 14 (21%) | 2 | 10 | 35 / 17 | 1.9 (+0.0) | 2.0 (+0.6) |
| [1, 1.1) | 74 | 30 (41%) | 2 | 10 | 20 / 24 | 1.1 (+0.2) | 1.5 (-0.3) |
| [1.1, 1.25) | 107 | 52 (49%) | 1 | 8 | 27 / 28 | 0.9 (+0.0) | 1.0 (+0.0) |
| [1.25, 1.5) | 120 | 78 (65%) | 1 | 2 | 21 / 21 | 0.5 (-0.1) | 0.5 (-0.1) |
| [1.5, 2) | 160 | 135 (84%) | 1 | 1 | 13 / 12 | 0.4 (-0.0) | 0.3 (-0.0) |
| [2, 99) | 181 | 179 (99%) | 1 | 1 | 2 / 0 | 0.3 (+0.0) | 0.3 (+0.0) |

By shape class and by whether limits A, B were given:

| shape class | rows | exact | med ulp | max | >/< | Excel abs err | ours abs err |
|---|---|---|---|---|---|---|---|
| a,b>1 | 1253 | 550 (44%) | 3 | 170 | 376 / 327 | 3.2 (+0.8) | 2.5 (+0.4) |
| a<=1<b | 277 | 220 (79%) | 2 | 12 | 37 / 20 | 0.6 (+0.0) | 0.7 (+0.1) |
| b<=1<a | 1207 | 192 (16%) | 4 | 146 | 570 / 445 | 4.3 (-0.1) | 4.3 (+1.0) |
| a,b<=1 | 85 | 41 (48%) | 1 | 9 | 17 / 27 | 0.8 (+0.0) | 1.1 (-0.2) |
| rows with limits A,B given | 600 | 232 (39%) | 2 | 38 | 188 / 180 | 2.2 (-0.2) | 1.9 (-0.2) |
| rows without limits | 2222 | 771 (35%) | 4 | 170 | 812 / 639 | 3.6 (+0.5) | 3.4 (+0.8) |

Shape inventory of the bank (so you know what has been asked): 1,521 distinct (a, b) pairs; a dense core of integer shapes 1..8 (about 12 x per pair), a ladder of fractional shapes
with many repeated values (0.05, 0.2, 0.3, 0.35, 0.45, 0.5, 1.7, 2.3, 4.6, 9.5, 16, 27, 31, 61, 96, 118, 200, 320 as the larger ones), and about 1,300 scattered fractional pairs;
x across 9e-10 .. 0.9999 (12 rows below 2^-8, none below 9e-10). Large shapes are rare: max(a,b) >= 100 occurs on 9 rows
with a, b > 1 (and on the b <= 1 rows with a = 118, 200, 320).

### Observations from the BETA.DIST data (ours; each is testable)

- **O1. The exact rows are where the answer is near one or the arm is a closed form.** b = 1 (exp(a ln x)): 111/111 exact;
  a = 1: 101/101 exact. For a, b > 1 with x above the mean (the swap arm, answer computed as 1 minus a small tail), 89% and 63%
  exact; below the mean 12-13%. Overall, Excel's answer < 0.5 is exact on 15% of 1,867 rows, answer >= 0.5 on 75%.
  The complement hides errors of the small side. **Excel and our code are the same when the answer is large and different when it is small.**
- **O2. The largest block (943 rows, a > 15, b <= 1, x > 0.5; answers mostly 4.5e-5 .. 0.6, a few far smaller), after the landed bgrat change.** Excel's mean abs error vs true is 4.7 ulp (signed -0.1); ours is now 4.6 (signed +1.3; it was 18.2 and -15.4 when bgrat ran at TOMS' 15 eps = 1.5e-14,
  a truncated positive series that stopped tens of ulp low). Only 75 of the 943 rows (8%) are exact, median miss 4 ulp, max 146; ours is above Excel on 511 rows, below on 357. So error scale and bias now agree and the shape does not:
  Excel's values here are unbiased and about 4.7 ulp from true, which a series stopped at 1e-15 reproduces only on average. Either Excel's expansion truncates at a similar size with different terms, or it uses another method for b <= 1
  (the power series in y = 1-x with the roles swapped, or a continued fraction, are alternatives). The 8 rows the change lost (they matched at 15 eps) are in Appendix C.
- **O3. Where Excel is the noisier one (a, b > 1, x below the mean; `L:bpser`, 247 rows):** Excel abs error 8.6 ulp with signed mean +3.4;
  ours 5.3 and +0.2. In the whole set of non-swapped series rows (733 rows), Excel's mean abs error rises with the size of the
  exponent: 1.6 ulp for -ln(result) < 1, 3.2 at 2-4, 6.7 at 4-8, 11.5 at 8-16, 29 at >= 40 (ours 1.9, 2.8, 4.0, 6.8, 27.8). An exponent
  of 40 stored in binary64 carries a rounding error of up to 3.5e-15, that is 16 to 32 ulp of the result, so **Excel's prefactor is an
  exp of a sum computed in binary64 whose terms are not compensated**; and the positive bias at 8-16 (+6.4 ulp mean for Excel,
  -1.0 for ours) says some term of that sum is systematically off.
- **O4. The anchors: Excel's LN and EXP leaves are ours.** BETA.DIST(x, a, 1) = exp(a * ln x) matches on all 111 rows, including
  BETA.DIST(2^-30, 1, 1), which Excel returns as x + 4 ulp (so exp(1 * ln x) is "exp of a binary64 logarithm", not the exact
  value). LN, EXP, POWER, LOG are exact in the bank (A.12), so the leaves are not the cause in the arms that are exact.
- **O5. Small-x, small integer shapes show structure.** BETA.DIST(x, 2, 2) = 3x^2 - 2x^3. At x = 2^-20 and 2^-10 Excel returns exactly the
  correctly rounded value 0x3d87ffff00000000 and 0x3ec7fc0000000000; at x = 2^-30 it returns 0x3c47ffffffc0000c, 12 ulp above the exact
  0x3c47ffffffc00000; ours is 3 below Excel at all three. (2,3): ours is +9, +9, -3. Both Excel and ours are far from "x^a times a rational";
  the error depends on the binade of x (the exponent), as an exp(a ln x + c) would.
- **O6. Complement structure.** Of the 71 pairs of bank rows (x, a, b) and (1-x, b, a) both present (limits absent), Excel's two answers sum to
  exactly 1.0 on 66 and to 1 - 2^-53 (0.99999999999999989) on 5. Consistent with Excel computing one side and returning 0.5 + (0.5 - w) for the other.

#### Sample of BETA.DIST rows (id | inputs | Excel bits and value | our bits | ulp(ours-Excel) | error vs true, Excel/ours)
Sampling: per region, mismatches spread evenly over the |ulp| ranking plus exact rows spread evenly over inputs; counts per region are in the tables above.

#### b = 1 arm (exp(a ln x))  (111 rows, 111 exact)
oc61 | x=9.313225746154785e-10 a=1.0 b=1.0 | Excel 0x3e10000000000004 9.3132257461547934e-10 | ours 0x3e10000000000004 | ulp +0 | true-err E/O 4.0/4.0

#### a = 1 arms  (101 rows, 101 exact)
oc1458 | x=0.021000569448971974 a=1.0 b=2.0 | Excel 0x3fa5475e0d521cd2 0.041560114980762855 | ours 0x3fa5475e0d521cd2 | ulp +0 | true-err E/O 0.2/0.2
oc191 | x=0.9375 a=1.0 b=10.0 | Excel 0x3fefffffffffe000 0.99999999999909051 | ours 0x3fefffffffffe000 | ulp +0 | true-err E/O 0.0/0.0

#### tiny x (x < 2^-8), every mismatching row of that kind in the bank  (12 rows, 3 exact)
oc67 | x=9.313225746154785e-10 a=0.5 b=0.5 | Excel 0x3ef45f306dd75d4e 1.9428093642163018e-05 | ours 0x3ef45f306dd75d4d | ulp -1 | true-err E/O 0.4/-0.6
oc68 | x=9.5367431640625e-07 a=0.5 b=0.5 | Excel 0x3f445f30a41cf5da 0.00062169909526915289 | ours 0x3f445f30a41cf5d9 | ulp -1 | true-err E/O 0.8/-0.2
oc69 | x=0.0009765625 a=0.5 b=0.5 | Excel 0x3f946009d2eee445 0.019897607325877215 | ours 0x3f946009d2eee444 | ulp -1 | true-err E/O 0.1/-0.9
oc64 | x=9.313225746154785e-10 a=2.0 b=2.0 | Excel 0x3c47ffffffc0000c 2.6020852123496281e-18 | ours 0x3c47ffffffc00009 | ulp -3 | true-err E/O 12.0/9.0
oc65 | x=9.5367431640625e-07 a=2.0 b=2.0 | Excel 0x3d87ffff00000000 2.7284823705953087e-12 | ours 0x3d87fffefffffffd | ulp -3 | true-err E/O 0.0/-3.0
oc66 | x=0.0009765625 a=2.0 b=2.0 | Excel 0x3ec7fc0000000000 2.859160304069519e-06 | ours 0x3ec7fbfffffffffd | ulp -3 | true-err E/O 0.0/-3.0
oc70 | x=9.313225746154785e-10 a=2.0 b=3.0 | Excel 0x3c57ffffff80000d 5.2041704214680828e-18 | ours 0x3c57ffffff800016 | ulp +9 | true-err E/O 13.0/22.0
oc71 | x=9.5367431640625e-07 a=2.0 b=3.0 | Excel 0x3d97fffe00000c01 5.4569612717461479e-12 | ours 0x3d97fffe00000c0a | ulp +9 | true-err E/O 1.0/10.0
oc72 | x=0.0009765625 a=2.0 b=3.0 | Excel 0x3ed7f800c0000000 5.7145980463246815e-06 | ours 0x3ed7f800bffffffd | ulp -3 | true-err E/O 0.0/-3.0

#### S:bgrat swap, a>15, b<=1, x>0.5 (the largest block)  (943 rows, 75 exact)
oc303 | x=0.9375 a=15.3 b=0.3 | Excel 0x3fb6b7e34619d827 0.088743404974451864 | ours 0x3fb6b7e34619d827 | ulp +0 | true-err E/O -3.0/-3.0
oc756 | x=0.9633087012715441 a=19.125912214583614 b=0.2 | Excel 0x3fb5b2fa043902e0 0.084762216597492834 | ours 0x3fb5b2fa043902e3 | ulp +3 | true-err E/O -4.6/-1.6
oc836 | x=0.9316606420979489 a=35.71739462342724 b=0.2 | Excel 0x3f7c176ddedda19f 0.0068582813090433395 | ours 0x3f7c176ddedda1a6 | ulp +7 | true-err E/O -1.4/5.6
oc241 | x=0.75 a=61.0 b=0.05 | Excel 0x3dd84b6296f7ce33 8.8382778302626493e-11 | ours 0x3dd84b6296f7ce34 | ulp +1 | true-err E/O 13.4/14.4
oc1426 | x=0.75 a=320.0 b=0.45 | Excel 0x375a78048df20f38 4.7476116623249035e-42 | ours 0x375a78048df20fca | ulp +146 | true-err E/O -31.4/114.6

#### S:bpser, min(a,b)<=1, no swap  (144 rows, 15 exact)
oc86 | x=0.125 a=0.3 b=0.7 | Excel 0x3fddb51a6622a5c1 0.46417865728525604 | ours 0x3fddb51a6622a5c1 | ulp +0 | true-err E/O 1.7/1.7
oc67 | x=9.313225746154785e-10 a=0.5 b=0.5 | Excel 0x3ef45f306dd75d4e 1.9428093642163018e-05 | ours 0x3ef45f306dd75d4d | ulp -1 | true-err E/O 0.4/-0.6
oc158 | x=0.25 a=50.0 b=0.2 | Excel 0x3948649be5d2c325 9.3958432945698425e-33 | ours 0x3948649be5d2c350 | ulp +43 | true-err E/O -12.5/30.5

#### S:bpser swap  (153 rows, 117 exact)
oc2614 | x=0.4564075504868651 a=0.6603993496736981 b=0.9124753182917799 | A=-0.2686788860132783 B=0.48333464289994266 | Excel 0x3feef55b08eb15f5 0.96745063535553177 | ours 0x3feef55b08eb15f6 | ulp +1 | true-err E/O -0.3/0.7

#### S:bpser_sym, S:bup20+bgrat, S:bgrat (both orientations)  (117 rows, 34 exact)
oc147 | x=0.0625 a=0.2 b=50.0 | Excel 0x3fefe838dfde273a 0.99709743236710513 | ours 0x3fefe838dfde273a | ulp +0 | true-err E/O -0.2/-0.2
oc2676 | x=0.46831836328156856 a=0.5295095857063042 b=1.6627658598213668 | A=0.10229139845374835 B=1.2375493327413358 | Excel 0x3fe63c13ddc5c472 0.6948336917395308 | ours 0x3fe63c13ddc5c475 | ulp +3 | true-err E/O 0.6/3.6
oc2787 | x=0.031147012170979815 a=0.9044182889485394 b=4.4889408286510015 | A=-0.6837056844412062 B=1.6631009290211325 | Excel 0x3fea837dd67e4241 0.82855121510699103 | ours 0x3fea837dd67e4242 | ulp +1 | true-err E/O 1.3/2.3
oc2291 | x=1.4102190683619622 a=7.630735376524125 b=0.6342331321374376 | A=-0.14515985115871555 B=1.9326997377353474 | Excel 0x3fabf0b0965e899c 0.054570692401529203 | ours 0x3fabf0b0965e89c2 | ulp +38 | true-err E/O -38.1/-0.1

#### L:bpser (a,b>1, x below the mean), power series  (247 rows, 30 exact)
oc2541 | x=1.654735549545311 a=1.0117491357556119 b=2.0538086557085724 | A=1.178740695655394 B=2.777954090595371 | Excel 0x3fe05c300163195b 0.51125335951727935 | ours 0x3fe05c300163195b | ulp +0 | true-err E/O -1.6/-1.6
oc392 | x=0.03 a=1.7 b=2.3 | Excel 0x3f8123f46c94982c 0.0083693595539210061 | ours 0x3f8123f46c94982b | ulp -1 | true-err E/O 0.7/-0.3
oc2311 | x=-0.023345759884139228 a=3.943498066224477 b=5.67153955664836 | A=-0.29245087984789464 B=2.590599488262995 | Excel 0x3f7901f756c2ba77 0.0061053907085538628 | ours 0x3f7901f756c2ba73 | ulp -4 | true-err E/O -0.5/-4.5
oc2195 | x=0.05045000508926023 a=8.0 b=8.0 | Excel 0x3e8a4dd0ec948f73 1.9597984239019877e-07 | ours 0x3e8a4dd0ec948f30 | ulp -67 | true-err E/O 54.0/-13.0
oc537 | x=0.11 a=9.5 b=2.3 | Excel 0x3e4b30ff6a53185b 1.2661981083755558e-08 | ours 0x3e4b30ff6a53185b | ulp +0 | true-err E/O 6.3/6.3

#### L:bpser swap  (259 rows, 231 exact)
oc2212 | x=0.08070806235620465 a=3.5578940500177714 b=2.3856782939477634 | A=-1.2731741256943159 B=0.23957077064365206 | Excel 0x3feeb99befb7cf50 0.96015736408160457 | ours 0x3feeb99befb7cf51 | ulp +1 | true-err E/O -0.3/0.7

#### L:bup_bpser (a,b>1), no swap  (342 rows, 43 exact)
oc2783 | x=-1.7042903591301948 a=1.6392585227543983 b=4.950853072704645 | A=-2.4057452351118958 B=0.5529086410825164 | Excel 0x3fe11b63ab81a2d1 0.53459342477170868 | ours 0x3fe11b63ab81a2ce | ulp -3 | true-err E/O 0.5/-2.5
oc2638 | x=1.4922390673656776 a=1.893528926582019 b=2.929793243278994 | A=0.7131110914903545 B=3.5368054975204557 | Excel 0x3fd4b9265d0b976a 0.32380065046742479 | ours 0x3fd4b9265d0b976a | ulp +0 | true-err E/O -1.2/-1.2
oc1696 | x=0.12618363572690838 a=3.0 b=6.0 | Excel 0x3fb1a6be416c5bcd 0.068950549099425731 | ours 0x3fb1a6be416c5bce | ulp +1 | true-err E/O 1.1/2.1
oc547 | x=0.52 a=9.5 b=4.6 | Excel 0x3fbcbb4d490a31d2 0.11223300011824414 | ours 0x3fbcbb4d490a31d2 | ulp +0 | true-err E/O -0.8/-0.8
oc608 | x=0.03 a=27.0 b=27.0 | Excel 0x3a712439b4ad5af8 3.4616994717430446e-27 | ours 0x3a712439b4ad5a4e | ulp -170 | true-err E/O 116.3/-53.7

#### L:bup_bpser swap  (371 rows, 235 exact)
oc1606 | x=0.3247600296003651 a=2.0 b=7.0 | Excel 0x3fe94bbb101fdba5 0.79049447202020373 | ours 0x3fe94bbb101fdba4 | ulp -1 | true-err E/O 0.4/-0.6

#### L:bup_bgrat (both) and L:bfrac (all)  (34 rows, 11 exact)
oc2231 | x=0.9032851391955108 a=1.0243594693048848 b=2.6945884291142406 | A=0.7436289822777225 B=1.2884736860704333 | Excel 0x3fe321cdd8ad592c 0.59787647552255896 | ours 0x3fe321cdd8ad592c | ulp +0 | true-err E/O 0.1/0.1
oc2133 | x=0.7978319126520609 a=8.0 b=2.0 | Excel 0x3fdb7ffda4eaeef6 0.42968693833584781 | ours 0x3fdb7ffda4eaeef7 | ulp +1 | true-err E/O -0.9/0.1
oc2141 | x=0.7044145021674233 a=8.0 b=3.0 | Excel 0x3fd941e7f2f5e62d 0.39464758612266132 | ours 0x3fd941e7f2f5e632 | ulp +5 | true-err E/O -2.2/2.8
oc138 | x=0.0625 a=30.0 b=40.0 | Excel 0x3c4352af96b19cd2 2.0950029656101305e-18 | ours 0x3c4352af96b19d1e | ulp +76 | true-err E/O -35.1/40.9

#### rows with limits A,B (z=(x-A)/(B-A))  (600 rows, 232 exact)
oc2518 | x=3.441079502252739 a=0.5001616747186722 b=0.6890562104262539 | A=1.6012541542852112 B=4.316766751846805 | Excel 0x3fe6edd1014f402e 0.71653032546783435 | ours 0x3fe6edd1014f402e | ulp +0 | true-err E/O -0.4/-0.4
oc2787 | x=0.031147012170979815 a=0.9044182889485394 b=4.4889408286510015 | A=-0.6837056844412062 B=1.6631009290211325 | Excel 0x3fea837dd67e4241 0.82855121510699103 | ours 0x3fea837dd67e4242 | ulp +1 | true-err E/O 1.3/2.3
oc2291 | x=1.4102190683619622 a=7.630735376524125 b=0.6342331321374376 | A=-0.14515985115871555 B=1.9326997377353474 | Excel 0x3fabf0b0965e899c 0.054570692401529203 | ours 0x3fabf0b0965e89c2 | ulp +38 | true-err E/O -38.1/-0.1

#### the three largest ulp differences in the bank
oc608 | x=0.03 a=27.0 b=27.0 | Excel 0x3a712439b4ad5af8 3.4616994717430446e-27 | ours 0x3a712439b4ad5a4e | ulp -170 | true-err E/O 116.3/-53.7
oc1426 | x=0.75 a=320.0 b=0.45 | Excel 0x375a78048df20f38 4.7476116623249035e-42 | ours 0x375a78048df20fca | ulp +146 | true-err E/O -31.4/114.6
oc560 | x=0.03 a=9.5 b=27.0 | Excel 0x3e8e515614ee1fed 2.2588461453761905e-07 | ours 0x3e8e515614ee1f6f | ulp -126 | true-err E/O 64.1/-61.9
oc609 | x=0.11 a=27.0 b=27.0 | Excel 0x3d687b6d8531e906 6.9582426638834826e-13 | ours 0x3d687b6d8531e960 | ulp +90 | true-err E/O -50.6/39.4

## A.4 BETAINV (62 rows; 12 shown evenly spread), FINV (32 rows; 16 shown), TINV (32 rows; the 8 at df = 25 shown)

Inputs are p then shapes. "err-vs-true-root" is how far each answer is from the true root of the exact forward function in ulps of the answer.
BETAINV all 62: Excel mean abs error vs the true root 0.85 ulp (max 9.3), ours 1.04 (max 6.8). FINV (the legacy right-tail inverse, our
`f_inv_rt_kernel`): Excel 2.4 (max 10.4), ours 3.7 (max 30.6). TINV (two-tailed): **Excel 9.9 (max 176), ours 2.3 (max 16.8).**
Our inverse bisects the double lattice until adjacent doubles, so it is the root of *our* forward; Excel's TINV answers are not
at the root of the exact forward, and the distance grows as p grows towards 1 (t small): p=0.02: 0.4 ulp, 0.07: 1.1, 0.15: 7.6, 0.3: 5.7, 0.6: 20.5,
0.85: 176 (df = 25). That is an absolute residual in p of about 1e-15 to 4e-15, so Excel's stopping rule (or its forward) has an absolute scale around 1e-15 in probability, not
a lattice stop. Microsoft's published documentation of BETAINV/FINV/TINV (older versions) describes an iterative search to a stated accuracy and #N/A after 100 iterations;
the answers here are far more accurate than the stated figure.

BETAINV rows (12 of 62):
```
oc2 | 0.001 0.5 0.5 | Excel 0x3ec4b2b2fc02d8cc | ours 0x3ec4b2b2fc02d8cb | ulp -1 | err-vs-true-root E/O 0.0/-1.0 
oc17 | 0.5 0.5 0.5 | Excel 0x3fdfffffffffffff | ours 0x3fe0000000000001 | ulp +2 | err-vs-true-root E/O -1.0/1.0 
oc32 | 0.02 0.75 1.25 | Excel 0x3f7170b4054afdde | ours 0x3f7170b4054afddc | ulp -2 | err-vs-true-root E/O 0.5/-1.5 
oc37 | 0.85 0.75 1.25 | Excel 0x3fe72f9b045e6d64 | ours 0x3fe72f9b045e6d63 | ulp -1 | err-vs-true-root E/O -0.3/-1.3 
oc43 | 0.3 1.5 2.5 | Excel 0x3fcd6b1fe71a28b1 | ours 0x3fcd6b1fe71a28b2 | ulp +1 | err-vs-true-root E/O 0.3/1.3 
oc4 | 0.01 2.0 3.0 | Excel 0x3fa580d85cf19313 | ours 0x3fa580d85cf19312 | ulp -1 | err-vs-true-root E/O 0.5/-0.5 
oc22 | 0.9 2.0 3.0 | Excel 0x3fe5bec972283f3e | ours 0x3fe5bec972283f3f | ulp +1 | err-vs-true-root E/O -0.9/0.1 
oc49 | 0.07 3.0 0.8 | Excel 0x3fdd4304a4a1b11c | ours 0x3fdd4304a4a1b11d | ulp +1 | err-vs-true-root E/O -0.0/1.0 
oc3 | 0.001 5.0 1.5 | Excel 0x3fcadb9f23eb37ca | ours 0x3fcadb9f23eb37c9 | ulp -1 | err-vs-true-root E/O 0.1/-0.9 
oc18 | 0.5 5.0 1.5 | Excel 0x3fe9872cd5508c05 | ours 0x3fe9872cd5508c05 | ulp +0 | err-vs-true-root E/O 0.4/0.4 =
oc56 | 0.02 6.0 2.0 | Excel 0x3fd9e16ca7f1b09f | ours 0x3fd9e16ca7f1b09e | ulp -1 | err-vs-true-root E/O -0.1/-1.1 
oc62 | 0.97 6.0 2.0 | Excel 0x3feeb4a1725f978d | ours 0x3feeb4a1725f978d | ulp +0 | err-vs-true-root E/O -0.1/-0.1 =
```
FINV rows (16 of 32):
```
oc17 | 0.003 1.0 12.0 | Excel 0x402b79e141bdda34 | ours 0x402b79e141bdda33 | ulp -1 | err-vs-true-root E/O -2.0/-3.0 
oc19 | 0.07 1.0 12.0 | Excel 0x400fa5981b55d3cf | ours 0x400fa5981b55d3d2 | ulp +3 | err-vs-true-root E/O -0.3/2.7 
oc21 | 0.3 1.0 12.0 | Excel 0x3ff2c6077c0014f5 | ours 0x3ff2c6077c0014f9 | ulp +4 | err-vs-true-root E/O -3.6/0.4 
oc23 | 0.85 1.0 12.0 | Excel 0x3fa31ea8bd82558d | ours 0x3fa31ea8bd825587 | ulp -6 | err-vs-true-root E/O 3.9/-2.1 
oc1 | 0.003 2.0 5.0 | Excel 0x4037084adac4b5d9 | ours 0x4037084adac4b5d8 | ulp -1 | err-vs-true-root E/O 1.5/0.5 
oc3 | 0.07 2.0 5.0 | Excel 0x4012f887a494b57a | ours 0x4012f887a494b57a | ulp +0 | err-vs-true-root E/O -0.8/-0.8 =
oc5 | 0.3 2.0 5.0 | Excel 0x3ff8beeba7b1552b | ours 0x3ff8beeba7b1552b | ulp +0 | err-vs-true-root E/O 1.1/1.1 =
oc7 | 0.85 2.0 5.0 | Excel 0x3fc57e5447b85738 | ours 0x3fc57e5447b85739 | ulp +1 | err-vs-true-root E/O -1.6/-0.6 
oc9 | 0.003 4.0 9.0 | Excel 0x40227d5c571ad768 | ours 0x40227d5c571ad76a | ulp +2 | err-vs-true-root E/O -1.4/0.6 
oc11 | 0.07 4.0 9.0 | Excel 0x4009480e31cae47e | ours 0x4009480e31cae47b | ulp -3 | err-vs-true-root E/O 1.1/-1.9 
oc13 | 0.3 4.0 9.0 | Excel 0x3ff6e644f3a0eb26 | ours 0x3ff6e644f3a0eb27 | ulp +1 | err-vs-true-root E/O 0.8/1.8 
oc15 | 0.85 4.0 9.0 | Excel 0x3fd5398e7f5c6a01 | ours 0x3fd5398e7f5c69fd | ulp -4 | err-vs-true-root E/O 2.0/-2.0 
oc25 | 0.003 7.0 3.0 | Excel 0x404f67e7e5e02ad1 | ours 0x404f67e7e5e02acb | ulp -6 | err-vs-true-root E/O 3.3/-2.7 
oc27 | 0.07 7.0 3.0 | Excel 0x401bb01322a0fe26 | ours 0x401bb01322a0fe20 | ulp -6 | err-vs-true-root E/O 4.7/-1.3 
oc29 | 0.3 7.0 3.0 | Excel 0x4000510f2252a65b | ours 0x4000510f2252a658 | ulp -3 | err-vs-true-root E/O 1.0/-2.0 
oc32 | 0.97 7.0 3.0 | Excel 0x3fc77788e3df0af3 | ours 0x3fc77788e3df0ae9 | ulp -10 | err-vs-true-root E/O 2.7/-7.3 
```
TINV rows (df = 25):
```
oc25 | 0.003 25.0 | Excel 0x400a4bc9075e5636 | ours 0x400a4bc9075e5636 | ulp +0 | err-vs-true-root E/O 1.0/1.0 =
oc26 | 0.02 25.0 | Excel 0x4003e17fdeeb8da5 | ours 0x4003e17fdeeb8da5 | ulp +0 | err-vs-true-root E/O 0.4/0.4 =
oc27 | 0.07 25.0 | Excel 0x3ffe496ee628f747 | ours 0x3ffe496ee628f747 | ulp +0 | err-vs-true-root E/O 1.1/1.1 =
oc28 | 0.15 25.0 | Excel 0x3ff7c34302131a39 | ours 0x3ff7c34302131a40 | ulp +7 | err-vs-true-root E/O -7.6/-0.6 
oc29 | 0.3 25.0 | Excel 0x3ff0ef24790dc55a | ours 0x3ff0ef24790dc555 | ulp -5 | err-vs-true-root E/O 5.7/0.7 
oc30 | 0.6 25.0 | Excel 0x3fe0ff363b6cd3d4 | ours 0x3fe0ff363b6cd3c0 | ulp -20 | err-vs-true-root E/O 20.5/0.5 
oc31 | 0.85 25.0 | Excel 0x3fc87592efa6421e | ours 0x3fc87592efa642cb | ulp +173 | err-vs-true-root E/O -176.3/-3.3 
oc32 | 0.97 25.0 | Excel 0x3fa372fe38f59e74 | ours 0x3fa372fe38f59e6a | ulp -10 | err-vs-true-root E/O -2.9/-12.9 
```

## A.5 FDIST (right tail, 4,399 rows) and F.DIST (cumulative, 369 rows)

Our route: Appendix B. FDIST and Excel's F.DIST.RT agree on all 6 + 4,142 controls (R33), so one computation.

Our bratio route taken (by the arguments v = d2/(d2+d1 x), w = d1 x/(d2+d1 x), shapes d2/2, d1/2):

| route in our bratio | rows | exact | median ulp of misses | max | ours>Excel / ours<Excel | Excel mean abs err vs true | ours |
|---|---|---|---|---|---|---|---|
| L:bup_bpser | 3147 | 444 (14%) | 3 | 19 | 1315 / 1388 | 1.9 | 2.5 |
| L:bpser | 901 | 125 (14%) | 3 | 23 | 377 / 399 | 5.2 | 2.0 |
| L:bup_bgrat | 283 | 45 (16%) | 2 | 6 | 82 / 156 | 1.2 | 1.5 |
| b1_exp | 19 | 13 (68%) | 6 | 11 | 3 / 3 | 1.7 | 2.3 |
| L:bup_bpser swap | 14 | 5 (36%) | 1 | 2 | 4 / 5 | 0.4 | 0.8 |
| a1_lo_expm1 | 12 | 12 (100%) | - | - | 0 / 0 | 1.0 | 1.0 |
| a1_hi | 10 | 8 (80%) | 6 | 6 | 1 / 1 | 0.9 | 0.6 |
| S:bpser swap | 6 | 4 (67%) | 1 | 1 | 2 / 0 | 0.4 | 0.3 |
| L:bpser swap | 6 | 5 (83%) | 1 | 1 | 0 / 1 | 0.6 | 0.4 |
| S:bpser | 1 | 0 (0%) | 6 | 6 | 0 / 1 | 2.0 | 8.0 |

The 51 (d1, d2) pairs have d1 in 1..13, d2 in 1..19 (most rows at (5,11) 828, (11,5) 581, (5,6) 569; exact 6-27% per pair); x runs from 0.006 to 166.7 but 90% of rows lie in 1.03 .. 3.55; 84% of answers are in 0.1-0.5, where 16% are exact (answers < 0.1: 9%; > 0.5: 40-90%).

Most FDIST rows are 15 dense windows of 257 consecutive doubles (x, x+1ulp, ... x+256ulp) at fixed (d1, d2), taken around variance ratios:

| window | (d1,d2) | first x (hex) | rows | distinct Excel answers | distinct our answers | exact |
|---|---|---|---|---|---|---|
| W1 | (5,11) | 0x3ff0e8335b1eb87b | 257 | 169 | 207 | 18 |
| W2 | (5,11) | 0x3ff8147473f6f1e8 | 257 | 148 | 142 | 44 |
| W3 | (5,11) | 0x40003d63ca3e93d8 | 257 | 206 | 184 | 24 |
| W4 | (5,6) | 0x3ff372a36987ef4e | 257 | 166 | 155 | 41 |
| W5 | (5,6) | 0x3fff2efbeb4df203 | 257 | 127 | 115 | 51 |
| W6 | (9,11) | 0x3ff06e3ac4f82314 | 257 | 212 | 209 | 18 |
| W7 | (9,3) | 0x3ffa0dee300cc803 | 257 | 145 | 137 | 39 |
| W8 | (6,5) | 0x3ffbb5e14f639d64 | 257 | 119 | 97 | 68 |
| W9 | (3,10) | 0x3ff2001b90783f08 | 257 | 162 | 137 | 44 |
| W10 | (7,7) | 0x3ff4c815cbf76347 | 257 | 172 | 183 | 27 |
| W11 | (11,5) | 0x4004554c6da5eb71 | 257 | 162 | 179 | 24 |
| W12 | (11,5) | 0x400c5ff0b9137953 | 257 | 99 | 105 | 2 |
| W13 | (8,9) | 0x3ff6242e39a79c46 | 257 | 183 | 175 | 24 |
| W14 | (5,3) | 0x4000c7057faae7e4 | 257 | 104 | 154 | 68 |
| W15 | (3,5) | 0x401227d70ecfac60 | 257 | 157 | 168 | 39 |

**A.6 The argument-resolution finding.** Within each window Excel's answer, as x steps by one ulp, should be a function of whatever
beta argument Excel forms. Our argument pair (v, w) = (d2/den, num/den) with num = d1*x and den = d2 + num, each rounded to binary64, collapses
neighbouring x: over the 15 windows there are 467 groups of x whose (v, w) are bit-identical, and **in 161 of those groups Excel's answers differ**
(the same computation of ours cannot tell them apart). Examples:

```
(d1,d2)=(5,11), v=d2/(d2+d1*x)=0.6755326018045846, w=d1*x/(d2+d1*x)=0.3244673981954154 (both computed in binary64):
    x=0x3ff0e8335b1eb966 (1.05668960183865)  Excel 0x3fdbc0e9d1c0ff3a  ours 0x3fdbc0e9d1c0ff35
    x=0x3ff0e8335b1eb967 (1.0566896018386502)  Excel 0x3fdbc0e9d1c0ff36  ours 0x3fdbc0e9d1c0ff35
(d1,d2)=(5,11), v=d2/(d2+d1*x)=0.5937931550669258, w=d1*x/(d2+d1*x)=0.40620684493307424 (both computed in binary64):
    x=0x3ff8147473f6f1e9 (1.5049938707226096)  Excel 0x3fd0faac0f78d894  ours 0x3fd0faac0f78d895
    x=0x3ff8147473f6f1ea (1.5049938707226098)  Excel 0x3fd0faac0f78d893  ours 0x3fd0faac0f78d895
(d1,d2)=(5,11), v=d2/(d2+d1*x)=0.5937931550669256, w=d1*x/(d2+d1*x)=0.40620684493307435 (both computed in binary64):
    x=0x3ff8147473f6f1ec (1.5049938707226103)  Excel 0x3fd0faac0f78d891  ours 0x3fd0faac0f78d891
    x=0x3ff8147473f6f1ed (1.5049938707226105)  Excel 0x3fd0faac0f78d88e  ours 0x3fd0faac0f78d891
```
We tested candidates for Excel's beta argument offline (exact rational arithmetic, round-to-nearest-even at 53 or 64 bits), counting groups of
window x with equal candidate and different Excel answers over the 3,855 window rows (0 = consistent with Excel). Conflicts: (v,w) in binary64: 161 (the pair above); v alone: 721-784; w alone: 791-863; (v, 1-v): 784 (binary64) / 260 (64-bit mantissa then rounded);
(v,w) from num and den kept at a 64-bit mantissa then rounded to 53 bits: 260; x/(x + d2/d1): 903; 1/(1+d1*x/d2): 601; d1*x/d2: 42; x*(d1/d2): 181. Consistent (0): any lossless function of x, including (v, w) kept unrounded at a 64-bit mantissa, or d1*x alone.
So **Excel's F-to-beta argument keeps more than binary64 division results of either tail, or the beta routine takes d1*x (or x) itself into the prefactor.**
Also, as x steps one ulp Excel's answer increases (non-monotone) on 183 of 3,840 steps, ours on 796: Excel is smoother in x than our (v, w) route,
(distribution of Excel's answer step per x step, in ulp: 0: 1,450; -1: 625; -2: 664; -3: 330; -4 or lower: 588; +1 or more: 183).

First 28 rows of two windows (x = base double + i; Excel bits, our bits, `=` if exact, ulp ours-Excel):

```

Window W1: FDIST(x, 5, 11), x = the double with bits 0x3ff0e8335b1eb87b + i, i = 0..256; showing i = 0..13
i=  0  Excel 0x3fdbc0e9d1c100f6  ours 0x3fdbc0e9d1c100fc   +6
i=  1  Excel 0x3fdbc0e9d1c100f4  ours 0x3fdbc0e9d1c100fc   +8
i=  2  Excel 0x3fdbc0e9d1c100f4  ours 0x3fdbc0e9d1c100f7   +3
i=  3  Excel 0x3fdbc0e9d1c100f0  ours 0x3fdbc0e9d1c100f5   +5
i=  4  Excel 0x3fdbc0e9d1c100f0  ours 0x3fdbc0e9d1c100f2   +2
i=  5  Excel 0x3fdbc0e9d1c100ed  ours 0x3fdbc0e9d1c100f0   +3
i=  6  Excel 0x3fdbc0e9d1c100e9  ours 0x3fdbc0e9d1c100f1   +8
i=  7  Excel 0x3fdbc0e9d1c100eb  ours 0x3fdbc0e9d1c100ee   +3
i=  8  Excel 0x3fdbc0e9d1c100e7  ours 0x3fdbc0e9d1c100ed   +6
i=  9  Excel 0x3fdbc0e9d1c100e6  ours 0x3fdbc0e9d1c100eb   +5
i= 10  Excel 0x3fdbc0e9d1c100e6  ours 0x3fdbc0e9d1c100ea   +4
i= 11  Excel 0x3fdbc0e9d1c100e2  ours 0x3fdbc0e9d1c100e8   +6
i= 12  Excel 0x3fdbc0e9d1c100df  ours 0x3fdbc0e9d1c100e2   +3
i= 13  Excel 0x3fdbc0e9d1c100de  ours 0x3fdbc0e9d1c100e2   +4

Window W13: FDIST(x, 8, 9), x = the double with bits 0x3ff6242e39a79c46 + i, i = 0..256; showing i = 0..13
i=  0  Excel 0x3fd45ac33ffc769d  ours 0x3fd45ac33ffc769e   +1
i=  1  Excel 0x3fd45ac33ffc769c  ours 0x3fd45ac33ffc7698   -4
i=  2  Excel 0x3fd45ac33ffc769c  ours 0x3fd45ac33ffc7698   -4
i=  3  Excel 0x3fd45ac33ffc769c  ours 0x3fd45ac33ffc7698   -4
i=  4  Excel 0x3fd45ac33ffc7698  ours 0x3fd45ac33ffc7698  =+0
i=  5  Excel 0x3fd45ac33ffc7697  ours 0x3fd45ac33ffc7698   +1
i=  6  Excel 0x3fd45ac33ffc7696  ours 0x3fd45ac33ffc7699   +3
i=  7  Excel 0x3fd45ac33ffc7693  ours 0x3fd45ac33ffc769b   +8
i=  8  Excel 0x3fd45ac33ffc7691  ours 0x3fd45ac33ffc768f   -2
i=  9  Excel 0x3fd45ac33ffc768f  ours 0x3fd45ac33ffc768a   -5
i= 10  Excel 0x3fd45ac33ffc768e  ours 0x3fd45ac33ffc768c   -2
i= 11  Excel 0x3fd45ac33ffc768d  ours 0x3fd45ac33ffc7692   +5
i= 12  Excel 0x3fd45ac33ffc768d  ours 0x3fd45ac33ffc768d  =+0
i= 13  Excel 0x3fd45ac33ffc7688  ours 0x3fd45ac33ffc7688  =+0
```

FDIST rows outside the windows (sample; 10 mismatches spread over |ulp| and 4 exact; the same format as the beta rows):

```
oc20 | x=1.0 d1=2 d2=2 | Excel 0x3fe0000000000000 0.5 | ours 0x3fe0000000000000 | ulp +0 | true-err E/O 0.0/0.0
oc62 | x=0.22 d1=2 d2=19 | Excel 0x3fe9bebf5fa3382c 0.80453461341108357 | ours 0x3fe9bebf5fa3382d | ulp +1 | true-err E/O -0.8/0.2
fd-var-ref-v3-046-0263 | x=1.9489707175811481 d1=5 d2=6 | Excel 0x3fcc2af477b4b078 0.22006088109026556 | ours 0x3fcc2af477b4b077 | ulp -1 | true-err E/O -0.4/-1.4
fd-var-ref-v3-022-0002 | x=1.504993870722524 d1=5 d2=11 | Excel 0x3fd0faac0f78da52 0.26529981146760406 | ours 0x3fd0faac0f78da50 | ulp -2 | true-err E/O 0.2/-1.8
fd-wrapper-ho-014-equal-forward | x=1.0 d1=6 d2=8 | Excel 0x3fdf0e2c0bbdd318 0.48523999354010661 | ours 0x3fdf0e2c0bbdd312 | ulp -6 | true-err E/O 0.3/-5.7
fd-var-v2-035-004 | x=2.8764419270502626 d1=11 d2=5 | Excel 0x3fc036d8e4dc8b3f 0.12667380499723355 | ours 0x3fc036d8e4dc8b3c | ulp -3 | true-err E/O 1.0/-2.0
fd-var-v2-019-007 | x=3.400884899521968 d1=11 d2=5 | Excel 0x3fb7f885e54a559d 0.093635910491079291 | ours 0x3fb7f885e54a55b1 | ulp +20 | true-err E/O -18.0/2.0
oc70 | x=0.61 d1=13 d2=6 | Excel 0x3fe92954dca03978 0.78629534808150847 | ours 0x3fe92954dca03978 | ulp +0 | true-err E/O -0.7/-0.7
```

F.DIST (cumulative, 369 rows; the most-used (d1, d2) pairs with counts: [((11, 5), 53), ((5, 6), 38), ((5, 11), 30), ((11, 4), 22), ((10, 3), 21), ((5, 3), 20), ((3, 5), 20), ((6, 5), 19), ((3, 10), 19), ((7, 7), 19)]):

| route in our bratio | rows | exact | median ulp of misses | max | ours>Excel / ours<Excel | Excel mean abs err vs true | ours |
|---|---|---|---|---|---|---|---|
| L:bup_bpser swap | 218 | 95 (44%) | 2 | 8 | 75 / 48 | 0.7 | 1.0 |
| L:bpser swap | 94 | 56 (60%) | 1 | 3 | 6 / 32 | 0.7 | 0.4 |
| L:bup_bgrat swap | 18 | 5 (28%) | 2 | 3 | 6 / 7 | 0.8 | 0.9 |
| L:bup_bpser | 11 | 3 (27%) | 2 | 6 | 4 / 4 | 0.3 | 1.5 |
| a1_hi | 10 | 8 (80%) | 1 | 1 | 1 / 1 | 0.4 | 0.4 |
| S:bpser swap | 9 | 7 (78%) | 1 | 1 | 0 / 2 | 0.2 | 0.2 |
| b1_exp | 8 | 6 (75%) | 3 | 3 | 1 / 1 | 1.1 | 0.8 |
| L:bup_bgrat | 1 | 0 (0%) | 1 | 1 | 1 / 0 | 0.5 | 1.5 |


## A.7 TDIST (92 rows: 46 inputs x tails 1 and 2; 16 of the 46 inputs shown, tails = 2)

Our route: bratio(df/2, 1/2, df/(df+x^2), x^2/(df+x^2)) (Appendix B). df takes 19 values 1..48; x 0.0625..7.5. 28/92 exact (14/46 per tail),
median miss 3 ulp, max 24. df = 5, 10 and 30 are exact on their 2 rows each; df = 2: 8/12. For df = 2 (a = 1 in the beta) the two-tailed value is 1 - x/sqrt(2+x^2); we tried eight spellings of it (1 - x/sqrt(2+x^2), 1 - sqrt(x^2/den), 1 - exp(0.5 ln(x^2/den)),
0.5 + (0.5 - exp(0.5 ln y)), 1 - x*den^-0.5, 1 - 1/sqrt(1+2/x^2), (sqrt(den) - x)/sqrt(den), with den = 2 + x^2) against the six df = 2 inputs (x = 0.3, 0.9, 1.7, 2.6, 4.2, 7.5): none fits all six
(at x = 0.3 all eight are 2 ulp below Excel; the spellings that fit x = 1.7 miss x = 2.6 and the converse; at x = 7.5 every one misses by 12-20 ulp). At x = 7.5 Excel is accurate (-0.2 ulp vs true) although the answer
0.0173 is a cancelling difference in those spellings, so Excel's df = 2 route is not a plain cancelling closed form. Over all rows Excel's mean abs error vs true is 3.7 ulp, ours 2.5.

```
oc2 | x=1.0 df=1 tails=2 | Excel 0x3fe0000000000001 0.50000000000000011 | ours 0x3fdffffffffffff8 | ulp -9 | true-err E/O 1.0/-8.0
oc42 | x=4.2 df=2 tails=2 | Excel 0x3faac4df53532520 0.052283266946299145 | ours 0x3faac4df53532520 | ulp +0 | true-err E/O -3.5/-3.5
oc24 | x=0.125 df=5 tails=2 | Excel 0x3fecf8fc6449a3b9 0.90539378726418029 | ours 0x3fecf8fc6449a3b9 | ulp +0 | true-err E/O 0.0/0.0
oc54 | x=4.2 df=6 tails=2 | Excel 0x3f774a00d2cf53dc 0.0056858093421014687 | ours 0x3f774a00d2cf53d9 | ulp -3 | true-err E/O 3.4/0.4
oc22 | x=0.25 df=10 tails=2 | Excel 0x3fe9d8410cc9fa92 0.80764820573661411 | ours 0x3fe9d8410cc9fa92 | ulp +0 | true-err E/O 0.7/0.7
oc66 | x=4.2 df=11 tails=2 | Excel 0x3f5856fe54ff4199 0.0014855846129543159 | ours 0x3f5856fe54ff4198 | ulp -1 | true-err E/O -3.0/-4.0
oc70 | x=0.3 df=17 tails=2 | Excel 0x3fe891efb53c3a44 0.76781449697029602 | ours 0x3fe891efb53c3a44 | ulp +0 | true-err E/O 0.1/0.1
oc80 | x=7.5 df=17 tails=2 | Excel 0x3ead27678508e454 8.6885462208871546e-07 | ours 0x3ead27678508e450 | ulp -4 | true-err E/O 6.3/2.3
oc84 | x=0.9 df=40 tails=2 | Excel 0x3fd7e78963c4422c 0.37350687735400601 | ours 0x3fd7e78963c4422c | ulp +0 | true-err E/O -0.9/-0.9
oc18 | x=4.0 df=48 tails=2 | Excel 0x3f2c9348de8643ef 0.00021801247136157785 | ours 0x3f2c9348de8643ea | ulp -5 | true-err E/O 7.9/2.9
```

## A.8 F.TEST (78 rows: 26 exact now; 52 mismatches, all within 11 ulp)

F.TEST(array1, array2) is the two-tailed probability that the variances differ: 2 * min(P(F > f), 1 - P(F > f)), f = var1/var2, degrees of freedom (n1-1, n2-1). Landed (2dfbcaf9): our kernel puts the larger variance first and now returns 2 * min(p, q) from the two
outputs of one bratio call (the F.DIST.RT staging); it used to return min(2 * RT, 1), which gave 1.0 on 13 rows (up to 1.8e15 ulp) whenever the right tail of the ratio exceeded 1/2, as happens with unequal degrees of freedom even at f = 1. Of those 13 rows 5 are now exact and 8 are 1-8 ulp off
(table below). Not swapping the ratio was tried and is worse (+6/-8 rows). The 44 other mismatches are 1-11 ulp; most use data shifted by 2^20, 2^40, 2^52 (rows named `translate_*`), where Excel's and our values differ from the exact value by the same
relative amount (up to 2e-7), so the variance estimator (mean, then sum of squared deviations, plain binary64 sums, statistical_tests_family.rs:5-25) behaves alike. Arrays have 3 to 12 elements; "equal-reverse" arrays are k copies of -1, one 0, k copies of +1 (shown abbreviated).
Whether Excel takes the smaller tail from p, from 1 - p, or from F.DIST's own complement is undecided at the last bit.

The 13 formerly gross rows, now (true = exact 2*min(p, 1-p)):

```
ft-wrapper-ho-015-equal-reverse | n=(9,7) arr1=[-1.0 -1.0 -1.0 ... 1.0 1.0] arr2=[-1.0 -1.0 -1.0 ... 1.0 1.0] | var1/var2=1 | true 0.97047998708021321 | Excel 0x3fef0e2c0bbdd312 | ours 0x3fef0e2c0bbdd31a | ulp +8
ft-var-v2-031-translate_2p40 | n=(5,12) arr1=[1099511627778.77 1099511627778.8757 1099511627780.5005 1099511627779.0535 1099511627777.2668] arr2=[1099511627776.5696 1099511627775.94 1099511627774.426 ... 1099511627777.5964 1099511627776.9392] | var1/var2=0.90776113 | true 0.9851416180780278 | Excel 0x3fef86479b403f10 | ours 0x3fef86479b403f12 | ulp +2
ft-var-v2-044-translate_2p52 | n=(5,12) arr1=[4503599627372001.0 4503599627371095.0 4503599627370297.0 4503599627369530.0 4503599627370392.5] arr2=[4503599627371786.0 4503599627369135.0 4503599627370958.0 ... 4503599627370114.0 4503599627371281.0] | var1/var2=0.99036778 | true 0.90491751158889133 | Excel 0x3fecf51231ebf772 | ours 0x3fecf51231ebf774 | ulp +2
ft-wrapper-ho-017-equal-reverse | n=(11,7) arr1=[-1.0 -1.0 -1.0 ... 1.0 1.0] arr2=[-1.0 -1.0 -1.0 ... 1.0 1.0] | var1/var2=1 | true 0.95069408416748047 | Excel 0x3fee6c15fffffffe | ours 0x3fee6c15fffffffc | ulp -2
ft-var-v2-001-moderate | n=(4,10) arr1=[-3.8203125 -6.09716796875 -7.849609375 -8.65771484375] arr2=[-2.67626953125 -3.978515625 0.17919921875 ... -6.9921875 -2.52734375] | var1/var2=0.95897041 | true 0.90597851177871935 | Excel 0x3fecfdc6a5defcba | ours 0x3fecfdc6a5defcb8 | ulp -2
ft-wrapper-ho-011-equal-reverse | n=(9,5) arr1=[-1.0 -1.0 -1.0 ... 1.0 1.0] arr2=[-1.0 -1.0 0.0 1.0 1.0] | var1/var2=1 | true 0.92181069958847739 | Excel 0x3fed7f7926fabb85 | ours 0x3fed7f7926fabb84 | ulp -1
ft-wrapper-ho-013-equal-reverse | n=(11,5) arr1=[-1.0 -1.0 -1.0 ... 1.0 1.0] arr2=[-1.0 -1.0 0.0 1.0 1.0] | var1/var2=1 | true 0.90311009868337178 | Excel 0x3fece64726510989 | ours 0x3fece6472651098a | ulp +1
ft-wrapper-ho-019-equal-reverse | n=(11,9) arr1=[-1.0 -1.0 -1.0 ... 1.0 1.0] arr2=[-1.0 -1.0 -1.0 ... 1.0 1.0] | var1/var2=1 | true 0.97989461264657074 | Excel 0x3fef5b4bf25afed9 | ours 0x3fef5b4bf25afeda | ulp +1
ft-wrapper-ho-001-equal-reverse | n=(5,3) arr1=[-1.0 -1.0 0.0 1.0 1.0] arr2=[-1.0 0.0 1.0] | var1/var2=1 | true 0.88888888888888884 | Excel 0x3fec71c71c71c71c | ours 0x3fec71c71c71c71c | ulp +0
ft-wrapper-ho-003-equal-reverse | n=(7,3) arr1=[-1.0 -1.0 -1.0 ... 1.0 1.0] arr2=[-1.0 0.0 1.0] | var1/var2=1 | true 0.84375 | Excel 0x3feb000000000001 | ours 0x3feb000000000001 | ulp +0
ft-wrapper-ho-009-equal-reverse | n=(7,5) arr1=[-1.0 -1.0 -1.0 ... 1.0 1.0] arr2=[-1.0 -1.0 0.0 1.0 1.0] | var1/var2=1 | true 0.95040000000000002 | Excel 0x3fee69ad42c3c9ee | ours 0x3fee69ad42c3c9ee | ulp +0
ft-wrapper-ho-005-equal-reverse | n=(9,3) arr1=[-1.0 -1.0 -1.0 ... 1.0 1.0] arr2=[-1.0 0.0 1.0] | var1/var2=1 | true 0.81920000000000004 | Excel 0x3fea36e2eb1c432e | ours 0x3fea36e2eb1c432e | ulp +0
ft-wrapper-ho-007-equal-reverse | n=(11,3) arr1=[-1.0 -1.0 -1.0 ... 1.0 1.0] arr2=[-1.0 0.0 1.0] | var1/var2=1 | true 0.80375514403292181 | Excel 0x3fe9b85cb5339f15 | ours 0x3fe9b85cb5339f15 | ulp +0
```
Two of the remaining mismatches:
```
ft-var-v2-029-translate_2p40 | n=(12,6) arr1=[1099511627776.4565 1099511627779.615 1099511627779.4438 ... 1099511627779.5361 1099511627779.9277] arr2=[1099511627777.4583 1099511627776.9556 1099511627777.263 1099511627777.4604 1099511627776.0928 1099511627776.1301] | var1/var2=3.5468458 | true 0.17316794336102445 | Excel 0x3fc62a5df7556dee | ours 0x3fc62a5df7556df9 | ulp +11
oc2 | n=(6,7) arr1=[3.1 4.2 5.9 2.6 8.4 7.1] arr2=[1.5 9.9 4.4 ... 5.5 2.2] | var1/var2=0.63882832 | true 0.63949193257336634 | Excel 0x3fe476b7c90eab97 | ours 0x3fe476b7c90eab98 | ulp +1
```

## A.9 NEGBINOM.DIST

Our route: mass by the Loader-type saddle point (matches 129/129 numeric mass rows); cumulative = regularized beta I_p(s, f+1) through the same bratio, except f = 0 (the mass).

| subset | rows | exact | median ulp of misses | max | ours>Excel / ours<Excel |
|---|---|---|---|---|---|
| mass (cumulative=FALSE) | 129 | 129 (100%) | - | - | 0 / 0 |
| cumulative, f=0 | 15 | 15 (100%) | - | - | 0 / 0 |
| cumulative, f>0, s=1 | 15 | 15 (100%) | - | - | 0 / 0 |
| cumulative, f>0, s>=2 | 111 | 51 (46%) | 2 | 183 | 30 / 30 |
| cumulative, f>0, s>=2, p<0.1 | 30 | 15 (50%) | 24 | 183 | 7 / 8 |
| cumulative, f>0, s>=2, 0.1<=p<0.5 | 51 | 10 (20%) | 1 | 32 | 20 / 21 |
| cumulative, f>0, s>=2, p>=0.5 | 30 | 26 (87%) | 3 | 4 | 3 / 1 |

All 60 mismatches are cumulative with f >= 1 and s >= 2. s = 1 (the a = 1 arm of the beta) is exact on all 15 rows with f >= 1. Sample (9 mismatches spread over |ulp|, 3 exact):

```
negbinom.dist-domain-00095-unit-endpoints | f=1 s=1 p=2.2250738585072014e-308 cum=TRUE | Excel 0x0020000000000000 4.4501477170144028e-308 | ours 0x0020000000000000 | ulp +0
negbinom.dist-domain-00259-unit-endpoints | f=7 s=2 p=2.220446049250313e-16 cum=TRUE | Excel 0x39c2000000000022 1.7749370367472885e-30 | ours 0x39c2000000000019 | ulp -9
negbinom.dist-domain-00189-unit-endpoints | f=2 s=2 p=0.25 cum=TRUE | Excel 0x3fd0c00000000000 0.26171875 | ours 0x3fd0c00000000001 | ulp +1
fresh-negbinom.dist-00127-fresh-endpoints | f=11 s=3 p=4.440892098500626e-16 cum=TRUE | Excel 0x36e6bfffffffffdd 3.1879540063389414e-44 | ours 0x36e6bffffffffff5 | ulp +24
negbinom-pin-001 | f=4 s=3 p=0.3999999999999999 cum=TRUE | Excel 0x3fe290257c914b14 0.58009599999999972 | ours 0x3fe290257c914b14 | ulp +0
w111-seed-recheck-NEGBINOM.DIST-007 | f=5 s=3 p=0.39999999999999997 cum=TRUE | Excel 0x3fe5e849aaeed68a 0.68460543999999968 | ours 0x3fe5e849aaeed68b | ulp +1
negbinom-pin-017 | f=6 s=3 p=0.4 cum=TRUE | Excel 0x3fe8953369a012ec 0.76821299200000004 | ours 0x3fe8953369a012ed | ulp +1
negbinom-pin-019 | f=4 s=3 p=0.4000000000000001 cum=TRUE | Excel 0x3fe290257c914b1b 0.5800960000000005 | ours 0x3fe290257c914b1a | ulp -1
fresh-negbinom.dist-00031-fresh-endpoints | f=3 s=7 p=0.375 cum=TRUE | Excel 0x3fa3a8e060000001 0.038397800177335746 | ours 0x3fa3a8e060000003 | ulp +2
fresh-negbinom.dist-00151-fresh-endpoints | f=11 s=7 p=0.375 cum=TRUE | Excel 0x3fe14809f858c395 0.54004381661626388 | ours 0x3fe14809f858c392 | ulp -3
fresh-negbinom.dist-00047-fresh-endpoints | f=3 s=11 p=4.440892098500626e-16 cum=TRUE | Excel 0x1d66bfffffffff68 4.8225175429138482e-167 | ours 0x1d66bffffffffeb1 | ulp -183
fresh-negbinom.dist-00175-fresh-endpoints | f=11 s=11 p=0.9999999999999999 cum=TRUE | Excel 0x3ff0000000000000 1 | ours 0x3ff0000000000000 | ulp +0
```

## A.10 HYPGEOM.DIST (numeric rows only)

Our route does not use the beta: the mass is a ratio of three binomial masses (Loader), cumulative is a sum of masses. Population N is at most 48 in the bank, so nothing here says what Excel does for larger populations.

| subset | rows | exact | median ulp of misses | max | ours>Excel / ours<Excel |
|---|---|---|---|---|---|
| mass (cumulative=FALSE) | 770 | 716 (93%) | 2 | 22 | 34 / 20 |
| cumulative | 769 | 734 (95%) | 2 | 22 | 20 / 15 |
| population N < 20 | 1309 | 1242 (95%) | 2 | 4 | 39 / 28 |
| population 20 <= N < 100 | 230 | 208 (90%) | 5 | 22 | 15 / 7 |
| Excel result < 0.01 | 570 | 565 (99%) | 1 | 7 | 3 / 2 |
| 0.01 <= result < 0.9 | 241 | 161 (67%) | 2 | 22 | 49 / 31 |
| result >= 0.9 | 728 | 724 (99%) | 2 | 2 | 2 / 2 |

```
hypgeom.dist-domain-00026-integer-support | s=0 n=0 K=0 N=0 cum=0 | Excel 0x3ff0000000000000 1 | ours 0x3ff0000000000000 | ulp +0
hypgeom.dist-domain-01414-integer-support | s=3 n=4 K=3 N=5 cum=0 | Excel 0x3fd9999999999999 0.39999999999999997 | ours 0x3fd999999999999b | ulp +2
hypgeom.dist-domain-01817-integer-support | s=1 n=2 K=2 N=6 cum=1 | Excel 0x3fedddddddddddde 0.93333333333333335 | ours 0x3feddddddddddddd | ulp -1
hypgeom.dist-domain-01826-integer-support | s=1 n=2 K=3 N=6 cum=0 | Excel 0x3fe3333333333336 0.60000000000000031 | ours 0x3fe3333333333334 | ulp -2
hypgeom.dist-domain-01911-integer-support | s=1 n=3 K=3 N=6 cum=1 | Excel 0x3fe0000000000000 0.5 | ours 0x3fe0000000000001 | ulp +1
hypgeom.dist-domain-01935-integer-support | s=3 n=3 K=5 N=6 cum=1 | Excel 0x3ff0000000000000 1 | ours 0x3ff0000000000000 | ulp +0
hypgeom.dist-domain-01997-integer-support | s=1 n=4 K=3 N=6 cum=1 | Excel 0x3fc999999999999b 0.20000000000000004 | ours 0x3fc9999999999998 | ulp -3
hypgeom.dist-domain-01988-integer-support | s=2 n=4 K=2 N=6 cum=0 | Excel 0x3fd9999999999997 0.39999999999999986 | ours 0x3fd9999999999998 | ulp +1
hypgeom.dist-domain-02074-integer-support | s=1 n=5 K=2 N=6 cum=0 | Excel 0x3fd5555555555553 0.3333333333333332 | ours 0x3fd5555555555555 | ulp +2
hypgeom.dist-domain-02100-integer-support | s=4 n=5 K=4 N=6 cum=0 | Excel 0x3fd5555555555553 0.3333333333333332 | ours 0x3fd5555555555554 | ulp +1
hypgeom.dist-branch-00152-fresh-raw-negative | s=4.25 n=12 K=6 N=22 cum=0 | Excel 0x3fd31b497b871393 0.2985404688191064 | ours 0x3fd31b497b871398 | ulp +5
fresh-hypgeom.dist-00993-fractional-support-controls | s=14.75 n=30.25 K=15.5 N=31 cum=1 | Excel 0x3fdef7bdef7bdee8 0.48387096774193461 | ours 0x3fdef7bdef7bdefe | ulp +22
hypgeom.dist-branch-00355-fresh-raw-negative | s=28 n=36.25 K=17 N=48 cum=1 | Excel 0x3ff0000000000000 1 | ours 0x3ff0000000000000 | ulp +0
```

## A.11 BINOM.DIST

| subset | rows | exact | median ulp of misses | max | ours>Excel / ours<Excel |
|---|---|---|---|---|---|
| mass (cumulative=FALSE) | 69335 | 69313 (100%) | 2 | 235 | 7 / 15 |
| cumulative, any | 1014 | 1007 (99%) | 335 | 651 | 0 / 7 |
| cumulative, 0<k<n (the beta route) | 14 | 7 (50%) | 335 | 651 | 0 / 7 |

All 29 mismatches (the 7 cumulative ones are the interior rows that reach the beta; the 22 mass rows are 1-18 ulp except two of 107 and 235 ulp at n = 610):

```
oc22420 | k=2 n=24 p=0.35000000000000064 cum=0 | Excel 0x3f65359d1668ad39 0.0025890415302193518 | ours 0x3f65359d1668ad43 | ulp +10
oc22736 | k=2 n=24 p=0.3500000000000182 cum=0 | Excel 0x3f65359d1668a1ca 0.0025890415302180824 | ours 0x3f65359d1668a1cf | ulp +5
oc22848 | k=2 n=24 p=0.3500000000000244 cum=0 | Excel 0x3f65359d16689dbb 0.0025890415302176318 | ours 0x3f65359d16689db0 | ulp -11
oc23116 | k=2 n=24 p=0.3500000000000393 cum=0 | Excel 0x3f65359d166893e4 0.0025890415302165393 | ours 0x3f65359d166893df | ulp -5
oc23812 | k=2 n=24 p=0.3500000000000779 cum=0 | Excel 0x3f65359d16687a95 0.0025890415302137295 | ours 0x3f65359d16687a8f | ulp -6
oc53280 | k=4 n=32 p=0.40000000000000957 cum=0 | Excel 0x3f42864051b5cd67 0.00056532040200211748 | ours 0x3f42864051b5cd70 | ulp +9
oc69559 | k=0 n=55 p=0.2311064565071698 cum=0 | Excel 0x3ea1b79145aacfdf 5.2800954660489616e-07 | ours 0x3ea1b79145aacff1 | ulp +18
oc68573 | k=447 n=610 p=0.5799338475220157 cum=0 | Excel 0x3cdaa83c3d623a56 1.4797701500866867e-15 | ours 0x3cdaa83c3d6239eb | ulp -107
oc67681 | k=500 n=610 p=0.5684265795910565 cum=0 | Excel 0x37cd65b4f2d6dde0 6.7492873116454142e-40 | ours 0x37cd65b4f2d6dcf5 | ulp -235
r20-binom-dist-013 | k=1 n=100 p=0.29999999999999993 cum=1 | Excel 0x3d0ff160cdebf6a3 1.4185489835068708e-14 | ours 0x3d0ff160cdebf695 | ulp -14
r20-binom-dist-017 | k=1 n=100 p=0.3 cum=1 | Excel 0x3d0ff160cdebf662 1.4185489835068605e-14 | ours 0x3d0ff160cdebf616 | ulp -76
r20-binom-dist-021 | k=1 n=100 p=0.30000000000000004 cum=1 | Excel 0x3d0ff160cdebf625 1.4185489835068509e-14 | ours 0x3d0ff160cdebf616 | ulp -15
r20-binom-dist-001 | k=1 n=1000 p=0.49999999999999994 cum=1 | Excel 0x020f48000000027c 9.3419688212178958e-299 | ours 0x020f48000000012d | ulp -335
r20-binom-dist-005 | k=1 n=1000 p=0.5 cum=1 | Excel 0x020f48000000027d 9.3419688212178968e-299 | ours 0x020f48000000012d | ulp -336
r20-binom-dist-009 | k=1 n=1000 p=0.5000000000000001 cum=1 | Excel 0x020f47fffffffaac 9.3419688212157738e-299 | ours 0x020f47fffffff95b | ulp -337
r20-binom-dist-035 | k=667 n=1000 p=0.98 cum=1 | Excel 0x024efd3fa03d6eaa 1.4807624475645133e-297 | ours 0x024efd3fa03d6c1f | ulp -651
```
Earlier measurement (R43): BINOM.DIST(k, n, p, TRUE) equals Excel's own BETA.DIST(1-p, n-k, k+1, TRUE) on 4,698 of 5,000 random calls; the 302 differences are p < 0.5, mostly in the lower tail k < np,
median 2 ulp, largest 1,436. BINOM.DIST(n-1, n, p, TRUE) with 1-p < 0.1 differs from BETA.DIST(1-p, 1, n, TRUE) on 91 of 500 calls and equals -expm1(n * log1p(-(1-p))) (binary64 product) on 499 of 500.
This says the binomial and the beta are two separate Excel routines that agree most of the time.

## A.12 Which related functions already match, and how

Counts (full bank, matched/rows): **exact or nearly:** LN 1,685/1,685; EXP 2,456/2,456; POWER 62,635/62,635; LOG 25,233/25,233; LOG10 2,823/2,823; ERFC 544/544; ERF.PRECISE 618,437/618,437;
NORM.DIST 24,246/24,246; NORM.S.DIST 27,491/27,497; LOGNORM.DIST 7,809/7,809; EXPON.DIST 49,345/49,346; WEIBULL.DIST 31,481/31,483; COMBIN 25,199/25,199; PERMUT 3,432/3,432;
BINOM.DIST.RANGE 1,224/1,224; CHISQ.DIST.RT 6/6; **GAMMA.DIST 77,808/77,975**; CHIDIST 372/388; POISSON.DIST 49,159/49,710; BINOM.DIST 70,320/70,349.
**Unmatched:** GAMMALN 2,137/4,538 (worst 7 ulp); GAMMALN.PRECISE 10,645/14,278; GAMMA 8,193/12,603 (worst 5); GAMMA.INV 63/204; CHIINV 37/100; CHISQ.INV 13/40; T.DIST.RT 1/6;
and the whole beta family above.

So the shared leaves LN, EXP, POWER and LOG are bit-exact, and **the incomplete gamma family is nearly exact (GAMMA.DIST 99.8%) even though GAMMALN and GAMMA are not**:
our gamma route uses a Loader-type saddle-point prefactor exp(-stirlerr(a) - deviance)/sqrt(2 pi a) with a Stirling-error table (n <= 6, values cut to 16 decimals) and a ten-term series from 7, with
the deviance's own series inside |x - np| < 0.2(x+np), not GAMMALN (discrete_dist.rs, Appendix B). The binomial, Poisson, negative binomial and hypergeometric masses use the same machinery
(70,320/70,349 and 3,572/3,661). A shared lgamma would propagate to anything that goes through it: the beta prefactor in our port does (TOMS betaln with its own gamln1/algdiv/bcorr),
and the pure-prefactor tests (R44, R45; Appendix C) refute every prefactor built from Excel's public GAMMALN, LN, POWER and EXP. **The natural hypothesis is that the beta prefactor is built from the Loader pieces
that the gamma, Poisson and binomial routines already use. We find no record in the round reports of that form having been tested for the beta** (Appendix C lists what was tried; R3's "beta-density identity" rejection concerned the binomial mass).

---

# Appendix B: our code

## B.1 The kernels and what routes where


**`crates/calc-func/src/ported/beta_gamma_stats_family.rs:31-82`** (BETA.DIST: z, the cumulative through bratio, and the density as a plain log form; BETA.INV: bit-converged bisection of the cumulative). **No density row exists in the bank**, so the density is unmeasured.
```rust
31: pub(crate) fn beta_dist_kernel(
32:     x: f64,
33:     alpha: f64,
34:     beta: f64,
35:     cumulative: bool,
36:     lower: f64,
37:     upper: f64,
38: ) -> Result<f64, WorksheetErrorCode> {
39:     validate_beta_shape(alpha, beta, lower, upper)?;
40:     if !x.is_finite() {
41:         return Err(WorksheetErrorCode::Value);
42:     }
43:     if x < lower || x > upper {
44:         return Err(WorksheetErrorCode::Num);
45:     }
46:     let z = (x - lower) / (upper - lower);
47:     if cumulative {
48:         // W109 lane-3 (2026-07-18, b30): Excel has NO integer-shape beta
49:         // fast path — bratio scores integer shapes at the same wall rate as
50:         // fractional (344/768 vs the binomial-sum shortcut's 254/768, and
51:         // the shortcut loses the disagreement rows 177:87). Route all
52:         // shapes through the identified BRATIO.
53:         return Ok(regularized_beta(z, alpha, beta));
54:     }
55:     let log_pdf = (alpha - 1.0) * crt_log(z) + (beta - 1.0) * crt_log(1.0 - z)
56:         - beta_log_norm(alpha, beta)
57:         - crt_log(upper - lower);
58:     let pdf = crt_exp(log_pdf);
59:     if pdf.is_finite() {
60:         Ok(pdf)
61:     } else {
62:         Err(WorksheetErrorCode::Num)
63:     }
64: }
65: 
66: pub(crate) fn beta_inv_kernel(
67:     probability: f64,
68:     alpha: f64,
69:     beta: f64,
70:     lower: f64,
71:     upper: f64,
72: ) -> Result<f64, WorksheetErrorCode> {
73:     validate_beta_shape(alpha, beta, lower, upper)?;
74:     if !probability.is_finite() {
75:         return Err(WorksheetErrorCode::Value);
76:     }
77:     if !(0.0..=1.0).contains(&probability) {
78:         return Err(WorksheetErrorCode::Num);
79:     }
80:     let z = bisect_inverse(probability, 0.0, 1.0, |v| regularized_beta(v, alpha, beta));
81:     Ok(lower + z * (upper - lower))
82: }
```

`ln_gamma` (special_math_common.rs:6-36) is a 9-coefficient Lanczos (g = 7) with `crt_log`; GAMMALN is a separate kernel (special_dist_family.rs:160) not used here.

`crt_log` / `excel_exp` are the identified Excel LN and EXP leaves (bit-exact against the LN and EXP banks). `crt_pow` is Excel's POWER.


**`crates/calc-func/src/ported/special_math_common.rs:2311-2337`** (eps, then the unit-shape arms of bratio, which were read from single-bit-stepped Excel calls)
```rust
2311:     let eps = eps0.max(1.0e-15);
2312:     // bgrat runs at `eps`, not TOMS 708's `15 * eps`: at 1.5e-14 its truncated positive series
2313:     // sat about 5 units low on average over the BETA.DIST bank (mean |ulp| 8.1, signed -5.3);
2314:     // at `eps` 3.9 and +0.3, with 31 rows gained and 8 lost (dc-ymjf.6).
2315:     // The unit-shape arms, read from single-bit-stepped public calls (Excel 16.0 b20527). With
2316:     // b = 1 and x <= 0.9 the answer is exp(a * log x), one rounding of each, on every x and shape
2317:     // asked; b = 1 +- 1 ulp leaves this arm, and just above 0.9 the answer is the complement of
2318:     // I_{1-x}(1, a). With a = 1 and x >= 0.1 it is that complement's own arm,
2319:     // 0.5 + (0.5 - exp(b * log y)); below 0.1 it is -expm1(b * log1p(-x)) with the
2320:     // product taken on the x87 chain (R43: 3,000 of 3,000 public calls, b from 0.3 to 1,000
2321:     // and x from 1e-9 to 0.0999; the spilled and internal EXPM1 graphs agree on all 20,000
2322:     // sampled arguments, so which of them Excel runs is undetermined).
2323:     if b == 1.0 && x <= 0.9 {
2324:         let w = crate::ported::excel_numeric::excel_exp(a * crt_log(x));
2325:         return (w, 0.5 + (0.5 - w));
2326:     }
2327:     if a == 1.0 && x >= 0.1 {
2328:         let w1 = crate::ported::excel_numeric::excel_exp(b * crt_log(y));
2329:         return (0.5 + (0.5 - w1), w1);
2330:     }
2331:     if a == 1.0 {
2332:         use crate::ported::excel_numeric::{excel_expm1_spilled, excel_log1p_times};
2333:         let w = -excel_expm1_spilled(excel_log1p_times(b, -x));
2334:         return (w, 0.5 + (0.5 - w));
2335:     }
2336:     if a.max(b) < 1.0e-3 * eps {
2337:         return (b / (a + b), a / (a + b));
```


**`crates/calc-func/src/ported/special_math_common.rs:2338-2450`** (dispatch; the bgrat calls now pass eps, not 15 * eps)
```rust
2338:     }
2339: 
2340:     let mut ind = 0;
2341:     let mut a0 = a;
2342:     let mut b0 = b;
2343:     let mut x0 = x;
2344:     let mut y0 = y;
2345:     if a0.min(b0) <= 1.0 {
2346:         // procedure for a0 <= 1 or b0 <= 1
2347:         if x > 0.5 {
2348:             ind = 1;
2349:             a0 = b;
2350:             b0 = a;
2351:             x0 = y;
2352:             y0 = x;
2353:         }
2354:         let branch: &str;
2355:         if b0 < eps.min(eps * a0) {
2356:             branch = "fpser";
2357:         } else if a0 < eps.min(eps * b0) && b0 * x0 <= 1.0 {
2358:             branch = "apser";
2359:         } else if a0.max(b0) > 1.0 {
2360:             if b0 <= 1.0 {
2361:                 branch = "bpser";
2362:             } else if x0 >= 0.3 {
2363:                 branch = "bpser_sym";
2364:             } else if x0 < 0.1 && crt_pow(x0 * b0, a0) <= 0.7 {
2365:                 branch = "bpser";
2366:             } else if b0 > 15.0 {
2367:                 branch = "bgrat_150";
2368:             } else {
2369:                 branch = "bup_140";
2370:             }
2371:         } else if a0 >= 0.2_f64.min(b0) {
2372:             branch = "bpser";
2373:         } else if crt_pow(x0, a0) <= 0.9 {
2374:             branch = "bpser";
2375:         } else if x0 >= 0.3 {
2376:             branch = "bpser_sym";
2377:         } else {
2378:             branch = "bup_140";
2379:         }
2380: 
2381:         match branch {
2382:             "fpser" => {
2383:                 w = bratio_fpser(a0, b0, x0, eps);
2384:                 w1 = 0.5 + (0.5 - w);
2385:             }
2386:             "apser" => {
2387:                 w1 = bratio_apser(a0, b0, x0, eps);
2388:                 w = 0.5 + (0.5 - w1);
2389:             }
2390:             "bpser" => {
2391:                 w = bratio_bpser(a0, b0, x0, eps);
2392:                 w1 = 0.5 + (0.5 - w);
2393:             }
2394:             "bpser_sym" => {
2395:                 w1 = bratio_bpser(b0, a0, y0, eps);
2396:                 w = 0.5 + (0.5 - w1);
2397:             }
2398:             "bup_140" => {
2399:                 let n = 20;
2400:                 w1 = bratio_bup(b0, a0, y0, x0, n, eps);
2401:                 b0 += n as f64;
2402:                 let (nw1, _ie) = bratio_bgrat(b0, a0, y0, x0, w1, eps);
2403:                 w1 = nw1;
2404:                 w = 0.5 + (0.5 - w1);
2405:             }
2406:             "bgrat_150" => {
2407:                 let (nw1, _ie) = bratio_bgrat(b0, a0, y0, x0, w1, eps);
2408:                 w1 = nw1;
2409:                 w = 0.5 + (0.5 - w1);
2410:             }
2411:             _ => unreachable!(),
2412:         }
2413:         if ind != 0 {
2414:             std::mem::swap(&mut w, &mut w1);
2415:         }
2416:         return (w, w1);
2417:     }
2418: 
2419:     // procedure for a0 > 1 and b0 > 1
2420:     let mut lambda_ = if a > b {
2421:         (a + b) * y - b
2422:     } else {
2423:         a - (a + b) * x
2424:     };
2425:     if lambda_ < 0.0 {
2426:         ind = 1;
2427:         a0 = b;
2428:         b0 = a;
2429:         x0 = y;
2430:         y0 = x;
2431:         lambda_ = lambda_.abs();
2432:     }
2433:     let branch: &str;
2434:     if b0 < 40.0 && b0 * x0 <= 0.7 {
2435:         branch = "bpser";
2436:     } else if b0 < 40.0 {
2437:         branch = "bup_160";
2438:     } else if a0 > b0 {
2439:         if b0 <= 100.0 {
2440:             branch = "bfrac";
2441:         } else if lambda_ > 0.03 * b0 {
2442:             branch = "bfrac";
2443:         } else {
2444:             branch = "basym";
2445:         }
2446:     } else if a0 <= 100.0 {
2447:         branch = "bfrac";
2448:     } else if lambda_ > 0.03 * a0 {
2449:         branch = "bfrac";
2450:     } else {
```


**`crates/calc-func/src/ported/special_math_common.rs:1665-1674`** (bpser, min(a,b) >= 1: prefactor exp(a ln x - betaln(a,b))/a; the arms for min(a,b) < 1 use gamln1/algdiv/gam1 forms and are omitted)
```rust
1665: fn bratio_bpser(a: f64, b: f64, x: f64, eps: f64) -> f64 {
1666:     let mut bp = 0.0;
1667:     if x == 0.0 {
1668:         return bp;
1669:     }
1670:     let a0 = a.min(b);
1671:     if a0 >= 1.0 {
1672:         let z = a * crt_log(x) - bratio_betaln(a, b);
1673:         bp = crate::ported::excel_numeric::excel_exp(z) / a;
1674:     } else {
```


**`crates/calc-func/src/ported/special_math_common.rs:1719-1739`** (bpser: the series 1 + a*sum, stopped when a term <= eps/a)
```rust
1719:         }
1720:     }
1721:     if bp == 0.0 || a <= 0.1 * eps {
1722:         return bp;
1723:     }
1724:     let mut summ = 0.0;
1725:     let mut n = 0.0;
1726:     let mut c = 1.0;
1727:     let tol = eps / a;
1728:     loop {
1729:         n += 1.0;
1730:         c = c * (0.5 + (0.5 - b / n)) * x;
1731:         let w = c / (a + n);
1732:         summ += w;
1733:         if w.abs() <= tol {
1734:             break;
1735:         }
1736:     }
1737:     bp * (1.0 + a * summ)
1738: }
1739: 
```


**`crates/calc-func/src/ported/special_math_common.rs:2048-2078`** (bgrat, the arm that takes the 943-row block: prefactor; its eps is now bratio's eps, 1e-15)
```rust
2048: fn bratio_bgrat(a: f64, b: f64, x: f64, y: f64, w: f64, eps: f64) -> (f64, i32) {
2049:     let mut c = [0.0f64; 31];
2050:     let mut d = [0.0f64; 31];
2051:     let bm1 = (b - 0.5) - 0.5;
2052:     let nu = a + 0.5 * bm1;
2053:     let lnx = if y > 0.375 {
2054:         crt_log(x)
2055:     } else {
2056:         bratio_alnrel(-y)
2057:     };
2058:     let z = -nu * lnx;
2059:     if b * z == 0.0 {
2060:         return (w, 1);
2061:     }
2062:     let mut r =
2063:         b * (1.0 + bratio_gam1(b)) * crate::ported::excel_numeric::excel_exp(b * crt_log(z));
2064:     r = r
2065:         * crate::ported::excel_numeric::excel_exp(a * lnx)
2066:         * crate::ported::excel_numeric::excel_exp(0.5 * bm1 * lnx);
2067:     let mut u = bratio_algdiv(b, a) + b * crt_log(nu);
2068:     u = r * crate::ported::excel_numeric::excel_exp(-u);
2069:     if u == 0.0 {
2070:         return (w, 1);
2071:     }
2072:     let (_p, q) = bratio_grat1(b, z, r, eps);
2073:     let v = 0.25 * crt_pow(1.0 / nu, 2.0);
2074:     let t2 = 0.25 * lnx * lnx;
2075:     let l = w / u;
2076:     let mut j = q / r;
2077:     let mut summ = j;
2078:     let mut t = 1.0;
```


**`crates/calc-func/src/ported/special_math_common.rs:2098-2109`** (bgrat: the series loop and its stopping rule)
```rust
2098:         let dj = d[n] * j;
2099:         summ += dj;
2100:         if summ <= 0.0 {
2101:             return (w, 1);
2102:         }
2103:         if dj.abs() <= eps * (summ + l) {
2104:             break;
2105:         }
2106:     }
2107:     (w + u * summ, 0)
2108: }
2109: 
```

The inverse searches (`search_upper_bound`, chi_f_t_family.rs:100-112) double an upper bound from max(start, 1) until forward(hi) >= target (at most 200 doublings) and then call `bisect_inverse` on [0, hi].
`betaln` (special_math_common.rs:1236-1302): bcorr/Stirling form for min(a,b) >= 8; gamln/gsumln combinations for 1 <= a < 8 with a product reduction of the larger shape; gamln(a) + algdiv(a,b) for a < 1, b >= 8; `gamln` is a rational gamln1 on [0.8, 2.25], reduction up to 10, a six-constant Stirling series above.

Not shown: `bup` (special_math_common.rs:1741-1797, the recurrence sum of 1-5 terms times brcmp1 / a with x87-multiplied terms), `brcomp` (1799-1899, x^a y^b / B(a,b): for min(a,b) >= 8 a Stirling-corrected form
with the bcorr correction, otherwise exp(a ln x + b ln y - betaln) with ln x or ln y taken as alnrel when x or y is below 0.375, with gam1 reductions below 1) and `bfrac` (2000-2046, Lentz-type continued
fraction times brcomp, stopped when |r - r0| <= eps r with eps = 15 * 1e-15). `exparg`, `esum`, `gam1`, `gamln1`, `algdiv`, `bcorr`, `gsumln` are the TOMS pieces with TOMS constants. The eps in effect is `max(2^-52, 1e-15)` = 1e-15 everywhere (special_math_common.rs:2311).

## B.2 F, t and the inverses


**`crates/calc-func/src/ported/chi_f_t_family.rs:166-224`** (F.DIST, F.DIST.RT = FDIST, F.INV, F.INV.RT = FINV; the pairs passed to bratio are (v, w), each a binary64 quotient)
```rust
166: pub fn f_dist_kernel(
167:     x: f64,
168:     deg1: f64,
169:     deg2: f64,
170:     cumulative: bool,
171: ) -> Result<f64, WorksheetErrorCode> {
172:     let x = validate_nonnegative_x(x)?;
173:     let d1 = truncate_positive_integer(deg1)?;
174:     let d2 = truncate_positive_integer(deg2)?;
175:     if cumulative {
176:         // W109: Excel's beta wrappers pass the ACCURATE complement pair, not
177:         // 1-z (identified stagings; b22 check).
178:         let num = d1 * x;
179:         let den = num + d2;
180:         Ok(bratio(d1 / 2.0, d2 / 2.0, num / den, d2 / den).0)
181:     } else {
182:         f_pdf_kernel(x, d1, d2)
183:     }
184: }
185: 
186: pub fn f_dist_rt_kernel(x: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
187:     let x = validate_nonnegative_x(x)?;
188:     let d1 = truncate_positive_integer(deg1)?;
189:     let d2 = truncate_positive_integer(deg2)?;
190:     // W109 identified staging: x = d2/den, y = d1*F/den (accurate complement).
191:     let num = d1 * x;
192:     let den = d2 + num;
193:     Ok(bratio(d2 / 2.0, d1 / 2.0, d2 / den, num / den).0)
194: }
195: 
196: pub fn f_inv_kernel(probability: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
197:     let p = validate_probability_open_unit(probability)?;
198:     let d1 = truncate_positive_integer(deg1)?;
199:     let d2 = truncate_positive_integer(deg2)?;
200:     let fwd = move |x: f64| {
201:         let num = d1 * x;
202:         let den = num + d2;
203:         bratio(d1 / 2.0, d2 / 2.0, num / den, d2 / den).0
204:     };
205:     let hi = search_upper_bound(p, 1.0, fwd);
206:     Ok(bisect_inverse(p, 0.0, hi, fwd))
207: }
208: 
209: pub fn f_inv_rt_kernel(probability: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
210:     let p = validate_probability_open_unit(probability)?;
211:     let d1 = truncate_positive_integer(deg1)?;
212:     let d2 = truncate_positive_integer(deg2)?;
213:     // Invert the published right-tail surface (f_dist_rt's accurate complement
214:     // form) directly at p, not the CDF at 1-p — same principle as CHIINV
215:     // (W109 b14+b19: the complement staging carries a systematic small-p bias).
216:     // The surface is decreasing in x, so invert the negated forward at -p.
217:     let f = move |x: f64| {
218:         let num = d1 * x;
219:         let den = d2 + num;
220:         -bratio(d2 / 2.0, d1 / 2.0, d2 / den, num / den).0
221:     };
222:     let hi = search_upper_bound(-p, 1.0, f);
223:     Ok(bisect_inverse(-p, 0.0, hi, f))
224: }
```

TDIST tails = 1: `0.5 * bratio(v/2, 0.5, v/den, t2/den).0`, tails = 2: the same without the 0.5 (v = trunc(df), t2 = x*x, den = v + t2; chi_f_t_family.rs:270-284). T.INV.2T = TINV (303-326): df = 1 closed form `1/tan(pi*p/2)` (p = 0.5 gives 1);
for df >= 2 the lattice bisection (`bisect_inverse`) of the negated two-tail forward `-bratio(v/2, 0.5, v/den, t2/den).0` at -p, upper bound found by doubling from 1.

**`crates/calc-func/src/ported/special_math_common.rs:2511-2543`** (bisect_inverse: bit-converged bisection on the double lattice, publishes hi)
```rust
2511: pub fn bisect_inverse<F>(target: f64, lo: f64, hi: f64, f: F) -> f64
2512: where
2513:     F: Fn(f64) -> f64,
2514: {
2515:     // Excel publishes fully-converged roots of its own forward (W109 agent-C
2516:     // verdict; the early-stop 4*EPS relative cutoff produced up to +1.9M-ULP
2517:     // errors at small roots on the b14 corpora). Bisect on the float lattice
2518:     // until lo and hi are adjacent doubles, maintaining f(lo) < target <= f(hi),
2519:     // and publish hi.
2520:     let key = |x: f64| -> i64 {
2521:         let i = x.to_bits() as i64;
2522:         if i < 0 { i64::MIN.wrapping_sub(i) } else { i }
2523:     };
2524:     let unkey = |k: i64| -> f64 {
2525:         if k < 0 {
2526:             f64::from_bits(i64::MIN.wrapping_sub(k) as u64)
2527:         } else {
2528:             f64::from_bits(k as u64)
2529:         }
2530:     };
2531:     let mut lok = key(lo) as i128;
2532:     let mut hik = key(hi) as i128;
2533:     while hik - lok > 1 {
2534:         let midk = lok + (hik - lok) / 2;
2535:         if f(unkey(midk as i64)) >= target {
2536:             hik = midk;
2537:         } else {
2538:             lok = midk;
2539:         }
2540:     }
2541:     unkey(hik as i64)
2542: }
2543: 
```

The catalogue maps TDIST(x, df, 1) to `t_dist_rt_kernel` (0.5 * bratio) and tails 2 to `t_dist_2t_kernel`; FINV to `f_inv_rt_kernel`; BETAINV to `beta_inv_kernel`; TINV to `t_inv_2t_kernel`
(crates/calc-func/src/catalogue/distributions.rs:30-80). df is truncated to an integer in all F and t kernels.

## B.3 Discrete distributions, F.TEST


**`crates/calc-func/src/ported/discrete_dist.rs:231-248`** (the binomial cumulative: the beta at q = 1-p with (n-k, k+1))
```rust
231: fn binom_cumulative(k: f64, n: f64, p: f64, q: f64) -> f64 {
232:     if k == n {
233:         1.0
234:     } else if k == 0.0 {
235:         binom_mass(k, n, p, q)
236:     } else if n - k == 1.0 && q < 0.1 {
237:         // One short of n with q below 0.1: -expm1(n * log1p(-q)) with the product stored in
238:         // binary64, not BETA.DIST's x87 chain at a = 1 (R43: Excel's BETA.DIST(q, 1, n) and
239:         // BINOM.DIST(n - 1, n, p, TRUE) differ on 91 of 500 fresh calls; this matches 499 of
240:         // the binomial's). Which EXPM1 graph runs is undetermined: none of the asked
241:         // arguments separates the internal and spilled ones.
242:         use crate::ported::excel_numeric::{excel_expm1_internal, excel_log1p};
243:         -excel_expm1_internal(n * excel_log1p(-q))
244:     } else {
245:         crate::ported::special_math_common::regularized_beta(q, n - k, k + 1.0)
246:     }
247: }
248: 
```


**`crates/calc-func/src/ported/discrete_dist.rs:445-470`** (NEGBINOM.DIST: cumulative = I_p(s, f+1) through bratio)
```rust
445: pub fn negbinom_dist_kernel(number_f: f64, number_s: f64, p: f64, cumulative: bool) -> R {
446:     let (f, s) = (number_f.trunc(), number_s.trunc());
447:     if f < 0.0 || s < 1.0 || !(p > 0.0 && p < 1.0) {
448:         return Err(NUM);
449:     }
450:     if !cumulative {
451:         return finite(negbinom_mass(f, s, p));
452:     }
453:     if f == 0.0 {
454:         return finite(negbinom_mass(0.0, s, p));
455:     }
456:     // Excel's cumulative is not the sum of its masses: at dyadic `p` it can sit on the exact dyadic
457:     // sum (`fresh-negbinom.dist-00113`) where masses through EXP cannot, and at `p = 1/8` it stands
458:     // 13 units off it. `I_p(s, f + 1)` holds the exact row and more of the 141 rows than the sum
459:     // (73 against 70). The stages belong to the incomplete beta, still open.
460:     finite(crate::ported::special_math_common::regularized_beta(
461:         p,
462:         s,
463:         f + 1.0,
464:     ))
465: }
466: 
467: #[cfg(test)]
468: mod tests {
469:     use super::*;
470: 
```

F.TEST uses sample_mean = sum / n and sample_variance = sum of (v - mean)^2 / (n - 1), both plain left-to-right binary64 sums (statistical_tests_family.rs:5-25).
The landed F.TEST kernel (statistical_tests_family.rs:51-85): ratio = larger variance / smaller (df swapped with it), num = df1*ratio, den = df2 + num, (p, q) = bratio(df2/2, df1/2, df2/den, num/den), result 2 * min(p, q); #NUM! if the ratio is not finite.

The binomial mass `binom_mass` and Poisson/gamma prefactor `poisson_mass` (discrete_dist.rs:80-229; header comment 1-40) are the Loader saddle-point form already described in A.12: exponent
`s(n) - s(x) - d(x, n p) - s(n-x) - d(n-x, n q)` stored after each subtraction (s = Stirling error: table for n <= 6, ten-term series from 7; d = deviance, own series inside |x - np| < 0.2(x + np), otherwise
x ln(x/np) + np - x with each step stored), normalizer `-ln n/2 + ln x/2 + ln(n-x)/2 + ln sqrt(2 pi)` as four extra running terms of the exponential, `excel_exp_running` (the exponential of a stored
difference with what the stores dropped put back as a factor). 100% of 13,000 rows of six (n, x) pairs sit on that exponent's grid.

---

# Appendix C: what earlier research concluded (and where it disagrees)

R = one of our research rounds. None found the shape; some conclusions are contradicted below.

- **Landed since the first version of this question (dc-ymjf.6).** (a) 2dfbcaf9, F.TEST = 2 * min(p, q) from one bratio call: 21 -> 26 of 78; the 13 formerly gross rows are 5 exact and 8 within 1-8 ulp (A.8); not swapping the ratio was tried and dropped (+6/-8).
  (b) 59703fa5, bgrat at eps (1e-15) instead of TOMS' 15 * eps in all three calls: BETA.DIST 982 -> 1,003 (29 gained, 8 lost), BETADIST 140 -> 142; BETA.DIST mean |ulp| to Excel 8.08 -> 3.86, mean signed -5.31 -> +0.29, rows within 1 ulp 1,354 -> 1,432. Tolerances tried: eps +23 net, 2^-52 +22, 1e-17 +21
  (so the result is nearly insensitive to the exact tolerance below 1e-15). **Lost rows, a lead for you** (they matched Excel at 15 eps and are now off; BETA.DIST(x, a, b, TRUE)): oc162 (0.75, 50, 0.2) +2 ulp; oc306 (0.9375, 15.3, 0.75) +1; oc1404 (0.9375, 96, 0.05) +12; oc279 (0.9375, 118, 0.95) +14;
  oc618 (0.9597, 17.49, 0.05) +1; and three rows with limits: oc2363 +1, oc2720 -1, oc2364 -4. Their exact-at-15-eps behaviour says Excel's series in some of these cases stops earlier than ours, or on a different test. (c) Unrelated to the beta, in the same fix pass (commit 4cc4ba08): the half-integer finite sum of the incomplete gamma now uses sqrt(pi) to binary64 (CHIDIST 371 -> 372, GAMMA.DIST 77,804 -> 77,808, CHIINV 30 -> 37).

- **W109 (early, "identified stagings").** Excel has no integer-shape beta fast path (bratio scores integer shapes like fractional: 344/768 against the binomial-sum shortcut's 254/768). The F and t wrappers "pass the accurate complement pair,
  not 1 - z". *Contradicted by the window finding A.6 (161 of 467 collision groups).*
- **R33.** FDIST equals F.DIST.RT on 4,142 controls; "1 - CDF" and all tested 1,920-row F-prefix / POWER programs are incomplete. Beta b = 1 integer arm matches 2,688 controls; "general I_x(2,2) prefactor/series models remain refuted".
- **R43 (research design note `incomplete-beta.txt`).** Fresh 14,400-call map (a, b in {0.3 .. 500}, 64 x per pair): b = 1 matches all; elsewhere 12-89% exact per pair, falling with a; largest differences about 10 ulp at a <= 3, 100-400 at a = 50, over 1,000 at a = 500 (those calls are not in the bank above).
  Error scale for a >= 20 (2,878 points): Excel median 12.8 ulp from true, TOMS port 15.6, plain lgamma prefactor + classic continued fraction 83.7 (correlation of Excel's errors with the port's 0.32, with the plain form 0.08). Ranked: (a) TOMS 708 with Excel's own leaves; (b) plain lgamma + continued fraction, refuted;
  (c) a = 1 closed forms (landed: -expm1 of the x87 log1p chain below x = 0.1, 3,000/3,000). Storing bpser's exponent, steps, product or division from x87 (16 combinations) changed none of 7,951 cells. Tiny-x prefactor alone (a, b in (1, 30), x in [1e-200, 1e-25]): the port's exp(a ln x - betaln)/a matches 990/1,500 (also with Excel's own GAMMALN/LN/EXP);
  x^a exp(-betaln)/a with Excel's POWER 1,081-1,098; x^a G(a+b)/(G(a)G(b))/a with Excel's GAMMA 1,029-1,040; 35 regroupings top out at 1,107.
- **R44-R45 (GPT-6.1 Sol).** 1,053 + 624 pure tiny-x controls (x <= 2^-60, 13 shape pairs in [2, 12]): for integer a, exact rescaling by 2^(e*a) gives different values as e changes, so **POWER(x, a) times a shape-only normalizer is refuted**; public-GAMMALN exponent forms match 18/1,053, public-POWER forms 59-63.
  A stored a*LN(x) with corrected subtraction lifts the TOMS normalizer from 18 to 262/1,053 (a diagnostic). Compensated subtraction fits (2,3), (4,7), (8,9), (8,8), (12,12) with one normalizer per pair, and no plain stored-exponent program fits all; the corrected interface s = store(p + C), exp(s) * store(1 + store((p - s) + C)) (192 variants) leaves a common C only at (2,3), (4,7), (8,9), (8,8).
  A 7,680-call map in adjacent 256-double windows around five exponent edges: public LN has 210 collision classes and none with differing beta answers (supports a stored-LN interface); stored product prefixes from resident LN, weighted FYL2X or base-2-then-ln2 each have 105 inconsistent classes of 216, at non-power-of-two a only
  (*consistent with the exact (2,2) rows at 2^-20, 2^-10 in A.3*). "Keeping a product remainder is still possible."
- **R46.** Binomial CDF: 221/774 stored-complement beta contradictions; preserving both probability operands still 6/774. Poisson and CHISQ.DIST.RT agree on 120 controls; 1 - GAMMA.DIST agrees on 76.
- **R49.** Counts unchanged; GAMMALN's reduction/join is implemented (1,348/1,348 fresh reductions on (3, 8)), no core table or constant recovered.
- **R3, R5, R20, R27-R31.** Rejected for the discrete masses: mode/neighbour recurrence, rational RLOG/RLOG1, beta-density and log-prefactor identities; for BINOM/NEGBINOM 9,000 direct arithmetic programs and five beta-CDF programs per name have no survivor; for HYPGEOM 72 product programs, four private-EXP programs, nine products and six recurrences are refuted, while the existing (a/c)*b order matches 94/94 dyadic single-draw controls.
- Named as open: the TOMS-style prefactor interface (normalizer, product, final scaling), the bgrat/bfrac arms, the F route's tail choice, the inverse solvers' stopping rule.

**Disagreements for you to settle.** (1) R43 ranks "TOMS 708 with Excel's leaves" first; the bgrat fix (a loose TOMS tolerance had been the whole bias, now removed) shows the tolerance was wrong but the arm still matches only 8% there, so the TOMS arm structure is not confirmed either. (2) R44-R45 inferred a stored-LN interface and a missing extra word in a product; A.6 shows the same symptom in the F path (more than binary64 of argument information).
(3) R43 read per-arm agreement of 29-60% as "a shared stage differs"; the data in A.3 say the arms differ in kind: accurate in one, noisier in another.

---

# Appendix D: the experiments we can run, and the tools

- Our Excel answer service takes a list of calls (function and numeric arguments, double bits exact) and returns each answer's 64 bits; thousands of calls per request, a few seconds. We can send adjacent-double windows, dyadic arguments, the same function through
  two spellings (BETADIST vs BETA.DIST, FDIST vs F.DIST.RT, TDIST vs T.DIST.2T) and compose worksheet formulas (e.g. compare BETA.DIST(x,a,b,TRUE) with 1 - BETA.DIST(1-x,b,a,TRUE) in Excel's own arithmetic, or with GAMMALN, EXP, LN, POWER leaves
  that are bit-exact).
- Offline tools: exact rational/multi-precision arithmetic and our bit-exact Excel LN, EXP, POWER; our Rust kernels with switchable stores.
- Absent or thin in the bank above (earlier rounds asked some of it outside the bank, Appendix C): BETA.DIST density; shapes above 380 or below 0.03; x below 9e-10; BETAINV/FINV/TINV beyond 62 + 32 + 32 inputs; F.TEST on larger arrays; TDIST with df above 48;
  BINOM.DIST interior cumulatives at moderate n and p (14 rows only); HYPGEOM at N above 48; the same beta call through both a literal and a worksheet-cell reference (to see whether the argument path matters).
