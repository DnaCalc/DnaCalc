# Harvest 10: beta, F and t family (answer 10)

Careful read of `pro/10-beta-family-answer.md` against `pro/10-beta-family-prompt.md`, pinned at
b2a80850 (the working tree differs from it only in docs, checked with `git diff --stat b2a80850 HEAD`).
Beads: dc-6332.4 (local replay, research interface), dc-6332.8 (Excel experiments), dc-6332.13 (deferred).

What was done beyond reading: the answer's exact-bit deductions were redone in Python, and every
candidate that needs no new Excel answer and no Rust build was replayed on the witness traces
(`codex-runs/pro-9/v2/trace-*`). Scratch scripts, all re-runnable, are in
`C:\Work\DnaCalc\codex-runs\pro-9\work\harvest-10\` (`arith.py`, `tinv_image.py`, `tinv_density.py`,
`tinv_z.py`, `tinv_small.py`, `finv_image.py`, `inv_image2.py`, `inv_image3.py`, `finv_z.py`, `finv_d2.py`,
`f_collide.py`, `f_collide2.py`, `f_unit.py`, `f_unit2.py`, `f_unit3.py`, `ftest_tie.py`, `ftest_tie2.py`,
`binom_k1.py`, `binom_probe.py`, `tdist_low.py`, `complement_pairs.py`). Elementary leaves in those
scripts: binary64 `+ - * /` and `sqrt` from Python (IEEE, round to nearest even); LN, EXP, LOG1P, EXPM1,
ATAN correctly rounded through mpmath at 300 bits, cross-checked with the Windows UCRT `math.log/exp`
(identical on every row used). Our Excel LN/EXP leaves are bit-exact on their banks, but where a
correctly rounded leaf and Excel's leaf can differ by one unit, the replay results below carry that caveat.
Nothing here ran cargo, Excel or a `br` write.

Headline findings from the replays (details in sections 2, 4 and 5):

- **TINV is decided for p <= 0.85 without Excel.** Excel's TINV value lies in the image of
  `SQRT(nu*(1/z-1))` over binary64 z on all 21 df >= 2 rows with p <= 0.85, at image densities down to
  1% or less, and the z that produces it is within 0.06-1.9 z-ulps of the true root (16 of 21 at the
  correctly rounded root). The 176-ulp row (TINV(0.85,25)) comes from z = 0x3feff40d65789bfd, 0.14 ulp
  from the true root: it is quantization through the transform, not a loose stopping rule. At p = 0.97
  (df 4, 9, 25) no scalar-z transform reproduces Excel, so a second route exists near p -> 1.
- **The answer's leading F candidates are rejected by the existing windows.** The C.5 separated-log
  state conflicts in 76 groups and the original-f log state in 74; `(v,w,LN f)` in 2, `(v,w,LN N)` in 93,
  `(v,w,LN D)` in 136. Survivors: `(RN(N/d2), RN(d2/N))` (0 of 301 collision groups), N alone (0 of 272),
  `(v,w,N)` (0 of 272). On the 47 unit-shape F rows the rounded quotient v into LN is best (35/47);
  separated logs, wider quotients and F-native logs all lose.
- **The concrete BINOM k = 1 spellings (C.7 A, B, C) match none of the six bank rows**; A and B sit 518
  ulps below Excel at n = 1000. The bank also holds a second, stronger q-collision witness the answer
  did not use (n = 100, p = 0.3 and 0.30000000000000004: same q, Excel 61 ulps apart).
- **The TDIST df = 2 and df = 1 closed forms of C.5 lose to the current code** (Q_A 0/6, Q_B 3/6,
  Q_C 0/6 against 4/6 now; both arctan spellings give exactly 0.5 at t = 1, Excel gives 0.5 + 1 ulp).
- **The complement question (E3) is mostly answered by the bank:** of 76 complementary BETA.DIST pairs,
  67 fit both graphs, 4 fit only `0.5+(0.5-w)`, none fits only `1-w`, 5 fit neither (independent arms).

---

## 1. Dispositions, section by section

| Answer section | Content | Disposition |
|---|---|---|
| Scope and principal recommendation | Expose wrapper information to the beta kernel; test the gamma/binomial arithmetic in beta prefactors; narrow three prompt conclusions | dc-6332.4 (research context of C.0). The three narrowings are correct (section 5). |
| A.1 "What is established" + unit-arm table | Unit-shape programs and their order | No action: identical to the code (special_math_common.rs:2323-2335). |
| A.1 uniform anchor `BETA.DIST(2^-30,1,1)` | Rejects an exact shortcut | No action: already the code's b = 1 arm. Used again in E1/E7 anchors (dc-6332.8). |
| A.1 "Separate the prefactor question" (S0 vs S1, P0 = C/y^b) | Two series with different prefactors | dc-6332.4 (C.2/C.3 variants). |
| A.1 axis table (normalizer / evaluator / precision) | Independent axes to vary | dc-6332.4 (shapes the C.0 interface). |
| A.1 Loader bridge (C = (b/n)M, log C) | Beta prefactor from the binomial-mass machinery | dc-6332.4 (C.2), first priority. |
| A.1 rounded-mean residual terms | `(mu_a+mu_b-n)+a log(nx/mu_a)+b log(ny/mu_b)` | dc-6332.4 (one more C.2 variant; the current masses already carry the rounded means, so this variant removes, not adds, that effect). |
| A.1 fractional-shape Stirling remark | "Reuse a real-shape helper only where you have it" | dc-6332.4: we have one (discrete_dist.rs:61-77); use it. The C.2 shift-to->=7 alternative: no action unless the measured helper fails (section 5, W9). |
| A.1 large-a small-b block (r, u/r, Q identities; three substitutions) | bgrat with shared gamma pieces | dc-6332.4 (C.4), second priority; E6 grid in dc-6332.8 after the replay. |
| A.1 "What the current evidence does not establish" (4 bullets) | Method cautions | No action: method notes; copy into the dc-6332.4/.8 briefs. |
| A.2 F mapping, collision logic, ranking 1-4 | State must carry more than (v, w) | dc-6332.4. Offline screen done here: ranks 1 (in the C.5 spellings) and 3 rejected, survivors listed (section 4, R7). |
| A.2 F prefactor expression and F-native deviance D | `n log1p((b/n)(f-1)) - b log f` | dc-6332.4 (C.5 F-native Loader; survives the collision screen because it uses f). |
| A.2 "Zero conflicts for RN(d1*f) ..." | E2 needs a colliding window | dc-6332.8 (E2). Understated, see W11. |
| A.2 F.TEST exact-variance control | 6-ulp gap to `2*FDIST(1,6,8)` | dc-6332.8 (E9) and dc-6332.4 (tie-orientation replay, section 4 R5). |
| A.3 TDIST mapping, half relation, df = 1, df = 2 forms, t^2 = 18 switch | t wrappers | df = 1/df = 2 closed forms: no action (rejected on the bank, W4). Switch: no action (already what the code does, W3). t/F/beta bridge: dc-6332.8 (E6). |
| A.4 inverses: coordinate first, error decomposition, lattice-bisection caveat | Architecture before tolerance | TINV: dc-6332.4 (section 4 R3, decided on the bank for p <= 0.85). FINV/BETAINV solver: dc-6332.13 (already deferred there). |
| A.5 discrete table | NEGBINOM/BINOM/HYPGEOM families | No action for NEGBINOM (code already is I_p(s,f+1)). BINOM: dc-6332.8 (E8). HYPGEOM: dc-6332.4 (C.7 variants on the bank) and dc-6332.13 (E11). |
| A.5 BINOM witness and the split prefix | q-collision rejects a q-only CDF | Confirmed, plus a second witness (D3). C.7 spellings rejected (W3). E8 windows in dc-6332.8. |
| A.5 HYPGEOM | Separate investigation | dc-6332.13 (E11; large N); recurrence replay dc-6332.4 (low). |
| A.6 beta density (three hypotheses, integer identity, endpoints, width) | Unmeasured | dc-6332.8 (E10 first part); C.8 programs in dc-6332.13 until E10 answers exist. |
| B "Conventions and how to read a negative result" | W(c,h), bit-exact inputs, record build/bitness/route, recheck old witnesses | dc-6332.8 (controls for every request). |
| E1 exact F arguments | Dyadic F/beta equality, the `2^30-1` anchor | dc-6332.8, first request. |
| E1 hypothesis table | rational `...000` vs beta unit `...004` | dc-6332.8 (frozen predictions; verified, section 2). |
| E2 RN(3f) collision window + offline states + d1 = 2 windows | Interface screen | Offline part done here (section 4 R7). Window: dc-6332.8 first request. d1 = 2 windows: dc-6332.8, sizes to fill in. |
| E3 complement probe + table | `RN(1-w)` vs two-stage | dc-6332.8 as a 15-call confirmation only; the bank already answers most of it (W6). Fill in a general-shape version (section 3). |
| E4 tiny-x interfaces | L, P, product-residual collisions | dc-6332.13: largely repeats R44/R45 (W7); keep only the product-residual arm, after C.2 lands or fails. |
| E5 branch identification + window table + Loader boundaries | Dispatch mapping | dc-6332.8 after dc-6332.4 has candidate programs to predict bits with; the Loader (3,5) windows can go in the first request as recorded data. |
| E6 bgrat grid, lost rows, t bridge, df = 2 window | | t bridge (24 calls): dc-6332.8 first request. bgrat grid and df = 2 window: dc-6332.8 after the C.4 replay. |
| E7 near-one quantization, image test, 0.85 neighbourhood, sharp controls, tails | Inverse architecture | Image test: done here (R3). Near-one batch: replace with a p ladder between 0.85 and 0.97 (section 3). 0.85 forward window and the four sharp controls: dc-6332.8 first request. Tolerance fitting: dc-6332.13. |
| E8 discrete identities + BINOM windows | 735 + 2,056 calls | dc-6332.8 first request. |
| E9 F.TEST matrix + permutations/translations | Tie convention, variance staging | Matrix: dc-6332.8 first request (all S_k pairs are exact ties, see section 3). Second part: dc-6332.13 until the matrix is read. |
| E10 density/mass identity, width, endpoints, affine | | Identity + endpoints: dc-6332.8 first request. Width and affine: dc-6332.8 second round. |
| E11 HYPGEOM beyond small N | 3/8 identities, long support | dc-6332.13 (as the bead already lists). See the proposed bead in section 6. |
| "Recommended first request" | Combined batch | dc-6332.8, amended as in section 3. |
| C.0 arithmetic contract, research context, seven independent stages | Interface | dc-6332.4 (Opus reviews the interface first, per the bead). |
| C.1 normalization, unit arms, complement1/2, control dispatch | | No action for the controls (identical to code). complement1 candidate: no action (bank, W6). Alternative fraction orientation: dc-6332.4 with E5. |
| C.2 Loader prefactors (mass_ratio, mass_scale, absorbed exponent, shift1, density_shift, P0 forms, residual variants, fractional s) | | dc-6332.4, first priority; fractional s: use the measured helper. |
| C.3 bpser control, positive series C*S1, bfrac with H = a*C, DLMF Lentz, recurrence check | | dc-6332.4 (bfrac-H and positive series after C.2; DLMF Lentz as a diagnostic only). |
| C.4 bgrat substitutions (gamma tail, gamma prefactor, gamma ratio g, l2 alternative) | | dc-6332.4, second priority, one substitution at a time. |
| C.5 F state, separated-log, original-f, log-beta H, F-native Loader, paired tails, wider candidate | | Separated-log and original-f: no action, rejected (W1). F-native Loader: dc-6332.4. Paired-tail F.TEST: dc-6332.4/.8. Wider: no action (rejected on unit rows, R8). |
| C.5 F.TEST baseline | Variance staging, tie convention | dc-6332.4 (tie replay R5). |
| C.5 TDIST and df = 1/2 programs | Q_A, Q_B, Q_C, arctan | No action: rejected on the bank (W4). |
| C.6 beta inverse scaling | Baseline | No action (identical to code). |
| C.6 FINV/TINV transforms | F_A..F_C, t_A..t_C | TINV t_C: dc-6332.4 (R3). FINV fixed transforms: no action (rejected, W5). |
| C.6 unit-shape and low-degree inverse candidates | b = 1, a = 1, d1 = 2, df = 1, df = 2 | d1 = 2 EXPM1 form: no action (2/8 against 3/8 now). df = 1: no action (TINV df = 1 already 8/8). b = 1/a = 1/df = 2 inverses: no bank rows; dc-6332.8 (E7 controls) then dc-6332.13. |
| C.6 iteration candidates, starts, reflection, tolerance-interval method | Solver harness | dc-6332.13 (already listed: "inverse architecture and solver harness"). |
| C.7 NEGBINOM | Beta context | No action (already the code). |
| C.7 BINOM k = 1 candidates A, B, C | | No action as replacements (rejected, W3); keep as named controls in the E8 analysis (dc-6332.8). |
| C.7 HYPGEOM mass (r, q = R(1-r)), orientation swap | | dc-6332.4, low; note it is not the code's q (W12). |
| C.7 HYPGEOM cumulative recurrence | Ascending/descending | dc-6332.4 on the bank (35 cumulative misses); seeds/crossover with E11 in dc-6332.13. |
| C.8 density programs and endpoints | | dc-6332.13 until E10 answers exist. |
| C.9 constants table | Known vs pending | No action: reference; agrees with the code except one omission (basym 100*eps, special_math_common.rs:2464) and the missing real-shape Stirling route. |
| C.10 implementation order | | Folded into section 7's top-five. |
| D.1 original operands, not a wider type | | No action (research brief); supported by R7/R8 here. |
| D.2 943-row block as gamma-helper mismatch | | dc-6332.4 (that is C.4). |
| D.3 TINV coordinate quantizer | | Confirmed for p <= 0.85 (R3); dc-6332.4. |
| D.4 common core, small front ends | | No action (brief). |
| D.5 invariants that reject whole programs | | No action (method; used throughout this file). |
| Source and evidence notes | A.11 prose correction; references | No action; the correction is right (D6). |

No section fits none of the three beads, except one product issue surfaced by E11 (section 6).

---

## 2. Claims checked against the pinned code and the prompt's data

### Code (b2a80850)

| # | Answer's claim | Verdict | Where, and what the code does |
|---|---|---|---|
| K1 | Unit arms: `b==1 && x<=0.9` first (wins at a = b = 1), then `a==1 && x>=0.1` two-stage complement, then `a==1` -EXPM1(log1p_times) | Agrees | special_math_common.rs:2323-2335 |
| K2 | The kernel receives y = RN(1-x) for public BETA.DIST | Agrees | special_math_common.rs:2507 (`regularized_beta`), beta_gamma_stats_family.rs:46,53 |
| K3 | bpser staging `R(b/j)`, `0.5+(0.5-r)`, `c*u*x`, `/(a+j)`, stop `|term|<=eps/a`, `P0*(1+a*sum)`; prefactor is x^a/(aB), so P0 = C/y^b | Agrees | special_math_common.rs:1672-1673, 1724-1737 |
| K4 | bgrat: r = z^b e^-z/Gamma(b), u/r = Gamma(a+b)/(Gamma(a) nu^b), Q from grat1, three separate EXPs, rounded z | Agrees (identities checked symbolically) | special_math_common.rs:2051-2077. Omitted by the answer: lnx is `alnrel(-y)` when y <= 0.375 (:2053-2057); every x of E6 has y <= 0.25, so C.4 must keep that path. |
| K5 | C.1 control dispatch: swap at x > 0.5 for min <= 1; lambda = R(R(n y)-b) if a > b else R(a-R(n x)) | Agrees | special_math_common.rs:2345-2353, 2420-2432 (n is formed inline as `(a + b)`) |
| K6 | eps = 1e-15, bgrat at eps, bfrac at 15*eps; constants 15, 20, 40, 100, 0.7, 0.03 | Agrees | :2311, :2402-2407, :2460, :2366-2369, :2434-2450. basym at 100*eps (:2464) is not in C.9. |
| K7 | Normalization width/offset/x and inverse `A + z*(B-A)` | Agrees | beta_gamma_stats_family.rs:46, :81 |
| K8 | Density: generic log form, -LN(width) inside the exponent, `0*log 0` at unit-shape endpoints | Agrees | beta_gamma_stats_family.rs:55-63: at x = A with alpha = 1 the exponent is NaN, so the code returns #NUM! where the mathematics gives b |
| K9 | F state N = R(d1 f), D = R(d2+N), v = R(d2/D), w = R(N/D); FDIST = I_v(d2/2,d1/2), F.DIST = I_w(d1/2,d2/2) | Agrees | chi_f_t_family.rs:178-180, :191-193 |
| K10 | TDIST: t2, D, v, w; two tails Q, one tail Q/2 | Agrees | chi_f_t_family.rs:270-284 (`0.5 *`, exact) |
| K11 | F.TEST: two-pass variance, larger variance first, tie to the first array, `2*min(p,q)` from one bratio call | Agrees | statistical_tests_family.rs:5-25, :60 (`var1 >= var2`), :81-84 |
| K12 | BINOM: q = R(1-p), the k = n-1, q < 0.1 EXPM1 arm, interior through the beta at q; C.7 "retain original p together with q" | Partly | discrete_dist.rs:221, :231-247. The code passes p and q to `binom_cumulative`, but `regularized_beta` re-forms y = R(1-q) (:2507), so original p never reaches the kernel; the answer's spec is a change, not the current state. |
| K13 | Mass core e0..e3 order and four-term normalizer (-LN n/2, LN a/2, LN b/2, ln sqrt(2 pi)) | Agrees | discrete_dist.rs:115-132; the stages are x87-emulated ops stored to binary64 (`excel_x87_sub/mul`), so the answer's R() equals the code except at double-rounding ties. Normalizer uses LN(n-x), not LN(b): the answer itself flags "retain b vs reconstruct n-a". |
| K14 | Deviance series threshold 0.2, strict | Agrees | x87_chains.rs:278-288 (gap `0.2*(x+np) - |x-np|` in x87 extended, strictly positive); discrete_dist.rs:82-93 |
| K15 | Stirling: table to 6 cut to 16 decimals, ten-term series | Agrees, incomplete | discrete_dist.rs:53-61. Missed: a non-whole shape takes the series at n+k in [6,7) and steps down (`stirling_error`, :68-77), measured on GAMMA.DIST shapes 1.1-7 (2,392/2,430). The C.2 "shift to >= 7" helper is a different program. |
| K16 | HYPGEOM `(a/c)*b` order retained | Agrees on order, disagrees on q | discrete_dist.rs:364-369: p = R(n/N), **q = R(R(N-n)/N)**; C.7 specifies q = R(1-r). They differ on 410 of 973 numeric 0 < n < N bank rows (49 of the 89 mismatches). |
| K17 | NEGBINOM cumulative I_p(s,f+1), f = 0 mass | Agrees | discrete_dist.rs:445-465 |
| K18 | (from the prompt) TINV df >= 2 is a lattice bisection | Agrees, but the prompt omitted a finding | chi_f_t_family.rs:321-326 bisects; :317-320 records W109's "Excel is SQRT(df*(1/BETA.INV(p,df/2,0.5)-1)) 135/135 vs the worksheet BETA.INV". The answer re-derived this as its first inverse hypothesis. |
| K19 | Upper-bound search kept as a control | Agrees | chi_f_t_family.rs:100-112 |

### The prompt's data

| # | Claim | Verdict |
|---|---|---|
| D1 | BETA.DIST(2^-30,1,1) = 0x3e10000000000004, 4 ulps above the input | Agrees (EXP(LN(2^-30)) = ...004 with correctly rounded leaves) |
| D2 | F.TEST(S_4,S_3) = 0x3fef0e2c0bbdd312, FDIST(1,6,8) = 0x3fdf0e2c0bbdd318, doubling exact (0x3fef0e2c0bbdd318); S_k variance exactly 1 under the two-pass code | Agrees. Also: the paired spelling F.DIST(1,8,6,TRUE) is 0x...318 too (fcdf-retired_wrapper-015), and the array-order twin ft-...-014 is 0x...318: F.TEST on a tie depends on array order (section 4 R5). |
| D3 | BINOM(1,1000,p-) and (1,1000,0.5) share q = 0.5; Excel 0x...27c / 0x...27d; exact 1001*2^-1000 = 0x020f480000000000; Excel 637 ulps above | Agrees (computed). Stronger witness in the bank: n = 100, p = 0.3 and 0.30000000000000004 both give q = 0x3fe6666666666666, Excel 0x3d0ff160cdebf662 vs 0x...f625 (61 ulps apart); ours gives one value for both. |
| D4 | TDIST df = 2: t = 4.2 and 7.5 lie on either side of the a = 1 switch (t^2 = 18) | Agrees, but this is already the code's route and both rows are exact now (oc41-44); the df = 2 misses are t = 0.3 (-2) and t = 2.6 (+8). |
| D5 | TINV df = 25 error 176.3 at 0.85, 2.9 at 0.97: not monotone toward 1 | Agrees; the prompt overstated "grows as p grows". |
| D6 | A.11's rows (1.4e-14, 9.3e-299, 1.5e-297) contradict A.2's "1e-150..1e-130" | Agrees: the prompt's prose was wrong. |
| D7 | BINOM has 14 interior cumulative rows | Agrees (14, 7 exact). |
| D8 | A.6 conflict counts | Reproduced here exactly: (v,w) 161 of 467 groups; N/d2 alone 42. |
| D9 | "Zero conflicts for RN(d1*f)" | Agrees, but N is not injective on the windows: 272 collision groups, all consistent (W11). |

### Exact-bit deductions redone (arith.py and others)

| Deduction | Result |
|---|---|
| E2: 3*f_i near 1.5; i = 2 is a midpoint rounding to the even upper product, i = 3 rounds down to it | Verified: both give 0x4012000000000002. Over the 257-input window, 64 groups of equal RN(3f) (193 distinct products). |
| E3: w = EXP(53.75*LN 0.5) = 1.1892*2^-54; RN(1-w) = 0x3fefffffffffffff; two-stage = 0x3ff0000000000000 | Verified. Of the five shapes only 53.5 and 53.75 separate the graphs; 53.25 gives ...fff for both, 54 and 54.25 give 1.0 for both. |
| E1 anchor: FDIST(2^30-1,2,2) arguments exactly (2^-30, 1-2^-30); rational 0x3e10000000000000, beta unit graph 0x3e10000000000004 | Verified; our code predicts ...004 (b = 1 arm). |
| E7: z = 1-2^(-2k) rounds to 1 for k >= 27 (tie at 27), with a 64-bit significand for k >= 33 | Verified. |
| E7: FINV(1-2^-53,2,2): (1-p)/p = 0x3ca0000000000001 (2^-53 plus one ulp), 1/p-1 = 2^-52 | Verified ("close to 2^-53" is exact to one ulp). |
| E7: RN(1-p/2) loses at k >= 53, RN(1-p) at k >= 54 | Verified. |
| E5: (3,5) Loader boundaries x = 1/16, 1/4, 9/16, 7/12; shifted density 2/9, 1/2, 5/9 | Verified by hand from 2/3 < mu/k < 3/2. In code the predicate uses binary64 0.2 in x87 (K14), so the floating boundaries sit near, not at, these rationals; the 257-double windows cover that. |
| E11: HYPGEOM(K-1,N-1,K,N,FALSE/TRUE) and HYPGEOM(1,1,K,N,FALSE) are exactly K/N = 3/8 | Verified. |
| Loader bridge C = (b/n)M and log C; rounded-mean residual and its first-order form (nx-a)(delta_a-delta_b) | Verified. |
| bgrat identities r, u/r, log(u/r), l2 = -log1p((b-1)/(2a)) | Verified. |
| F-native deviance D = n log1p((b/n)(f-1)) - b log f | Verified (v + w = 1). |
| Positive series ratio (n+j-1)/(a+j) x; DLMF fraction coefficients; Lentz updates; Halley step; t and F log-derivatives; df = 2 and d1 = 2 inverse formulas; E10 density identity | Verified. |
| TINV amplification at 0.85, df 25 | A half-ulp error in z moves t by at most 131 t-ulps, so 176 needs z about 0.67 z-ulp off; the image test finds Excel's z 0.14-1.14 ulp from the root (consistent). |

Not checked: the Boost documentation claims (no web access here); DLMF equation numbers beyond the
formulas themselves.

---

## 3. Proposed Excel experiments (dc-6332.8 unless stated)

Our code's prediction is listed where it is a fixed computation, so each request also scores the current code.

| Exp | Inputs as given | Excel calls | Predictions per hypothesis | Status |
|---|---|---|---|---|
| E1 main | k = 3..7, integer (d1,d2) with d1+d2 = 2^k; FDIST(1,d1,d2), F.DIST(1,d1,d2,TRUE), BETA.DIST(d2/2^k,d2/2,d1/2,TRUE), BETA.DIST(d1/2^k,d1/2,d2/2,TRUE) | All pairs: 243 pairs x 4 = 972 | Shared kernel differing only in argument precision: FDIST == first BETA.DIST and F.DIST == second, bit for bit. Mismatch: another difference (prefactor, evaluator, orientation). Our code: equal everywhere (same bratio call). | Ready (state "all pairs" or a subset) |
| E1 anchor + ladder | FDIST(2^30-1,2,2), BETA.DIST(2^-30,1,1,TRUE); FDIST(2^k-1,2,2) and BETA.DIST(2^-k,1,1,TRUE), k = 2..40 | 2 + 78 | Rational: 2^-k exactly; beta unit graph: EXP(LN 2^-k) (0x3e10000000000004 at k = 30). Our code: the beta unit graph. | Ready |
| E2 window | f_i = from_bits(0x3ff8000000000000+i), i = 0..256; FDIST(f_i,3,5) | 257 | Any state depending only on RN(3f): equal answers within its 64 collision groups (i = 2,3 first). Unequal rejects that interface. | Ready |
| E2 d1 = 2 windows | d2 in {3,5,11,16,31}, windows around f = d2/2 | 5 x 257 = 1,285 if 257 each | Unit-shape prefactor exposure; denominator FMA cannot help when 2f is exact. | Fill in window size |
| E2 offline states | (v,w,LN f), (v,w,LN N), (v,w,LN D), (v,w,lv,lw), (RN(N/d2),RN(d2/N)) on W1-W15 | 0 | Conflict = rejected. | **Done here** (section 4 R7) |
| E3 | BETA.DIST(0.5,s,1,TRUE), BETA.DIST(0.5,1,s,TRUE), worksheet 1-w; s in {53.25,53.5,53.75,54,54.25} | 15 | c = 0x3fefffffffffffff under RN(1-w), 0x3ff0000000000000 under two-stage, at s = 53.5, 53.75 only. Our code: two-stage. | Ready, low value: both calls run the unit arms, already 89/89 + 111/111. To test general arms, add pairs (x,a,b), (1-x,b,a) with a, b > 1 whose small tail is in (2^-54, 1.5*2^-54) (e.g. a = b = 20, x chosen offline): fill in. |
| E4 | 9 shape pairs, x = 2^-e, e = 40..80; CDF, density, LN | 9 x 41 x 2 + 41 = 779, plus windows | Constancy within L, P or (P, remainder) collisions. Pure powers of two give no L collisions; only the windows test it. | Needs windows and offline series-bound certification; P-level arm already done by R45 (W7). dc-6332.13. |
| E5 TOMS boundaries | a in W(15,128) at x = 15/16, b = 1/2; b in W(40,128) at x = 1/32, a = 3; a = b in W(100,128) at x = 511/1024; x in W(RN(0.0875),128) at (5,8); windows near a/(a+b) and (a+1)/(a+b+2) for (2,5),(5,2),(3,7); unit neighbours, .1/.9 windows; (120,140) around lambda = RN(0.03*120) | 4 x 257 + 6 x 257 + (120,140) 257 + unit windows: about 2,800 | Each candidate dispatch must match its two arms' bit signatures on both sides. Our routing at these points was checked: a <= 15 goes bup(20)+bgrat, a > 15 bgrat; b < 40 bup+bpser, b >= 40 bfrac; a = b <= 100 bfrac, > 100 basym. | Needs candidate programs (from dc-6332.4) to give bits; window sizes for the mean windows and unit windows |
| E5 Loader | (3,5) at x = 1/16, 1/4, 9/16, RN(7/12): CDF, density, BINOM masses; shifted boundaries 2/9, 1/2, 5/9 | 4 x 257 x 4 + 3 x 257 x 2 = 5,654 | Arm signatures switch at the deviance-series boundary if the Loader machinery is shared. | Inputs ready; predictions need C.2 implemented. Can be asked as data. |
| E6 bgrat grid | a in {15,16,32,50,96,118,200,320}, b in {1/16,1/4,1/2,3/4,15/16}, x in {3/4,...,127/128}; the 8 lost rows; windows around "the five fully specified ones"; gamma controls | 240 + 8 + unspecified | Full predicted bit vectors from each C.4 substitution with the expansion fixed. Note: at x = 127/128 some (a,b) route to bpser, not bgrat, in our code ((x0 b0)^a0 <= 0.7). | After the C.4 replay; fill in the windows |
| E6 t bridge | nu = 2^k-1, k = 1..8: TDIST(1,nu,2), FDIST(1,1,nu), BETA.DIST(nu/(nu+1),nu/2,1/2,TRUE); extension t = 2^j, nu = 2^k-2^(2j) | 24 + extension | Pure normalization difference: equal; mismatch rejects. Our code: equal (same bratio call). | Ready (extension ranges to fill in) |
| E6 df = 2 window | TDIST on W(RN(sqrt 18),128) | 257 (x2 with tails 1) | Switch of the a = 1 arms at the rounded predicate. | Ready; the closed forms it would compare are already rejected (W4) |
| E7 near-one | p_k = 1-2^-k, k = 20..52: TINV at 2 and 25, FINV(p,1,2), FINV(p,1,25), BETAINV in both orientations | 33 x 8 = 264 | Scalar binary64 z: zero or a positive floor for k >= 27 (df 2); 64-bit z: k >= 33; small coordinate: positive, decreasing. | Already decided for p = 0.97 at df 4, 9, 25 (scalar z rejected, R3). Replace most of this batch with a **p ladder between 0.85 and 0.97** at df 4, 9, 25 (e.g. 0.85 + j/400, 49 values x 3 df x TINV + BETAINV(p,nu/2,1/2) = 294) to locate the route change; keep k = 20..52 at df 2 and 25 (66) as the floor test. |
| E7 0.85 neighbourhood | p in W(0.85,128): TINV(p,25), FINV(p,1,25), BETAINV both orientations; TDIST and T.DIST.2T at t_j = from_bits(0x3fc87592efa6421e+j), j = -512..512, and at 0x3fc87592efa642cb | 1,028 + 2,050 + 2 | Plateau / local crossing / neither of the public forward. | Ready; R3 already predicts TINV(p,25) = SQRT(25*(1/z-1)) with z at or next to RN(root) across the window |
| E7 sharp controls | BETAINV(2^-30,1,1); FINV(1-2^-53,2,2); p = 2^-k, k = 48..80: TINV at {1,2,5,25}, FINV at (1,5),(2,5),(5,2) | 1 + 1 + 33 x 7 = 233 | BETAINV: identity 0x3e10000000000000, EXP(LN p) 0x3e10000000000004, or a third value (our bisection gives a third value). FINV: 1/p-1 = 2^-52 vs (1-p)/p = 2^-53+1ulp. Tails: RN(1-p/2) route loses at k >= 53, RN(1-p) at k >= 54. | Ready |
| E8 identities | f, s in {1,2,3,7,11,31,63}, p in {1/8,3/8,1/2,5/8,7/8}: NEGBINOM.DIST(f,s,p,TRUE), BETA.DIST(p,s,f+1,TRUE), BINOM.DIST(f,f+s,1-p,TRUE) | 735 | NB = BETA != BINOM: BINOM-specific stage; NB = BINOM != BETA: public-beta stage. Our code: all three identical on all 245 (same bratio call; no special arm applies). | Ready |
| E8 BINOM windows | 257 doubles around 0.5 (n = 1000) and 0.3 (n = 100): BINOM(1,n,p,TRUE), BINOM(0,n,p,FALSE), BINOM(1,n,p,FALSE), BETA.DIST(RN(1-p),n-1,2,TRUE) | 2,056 | q-only: constant within q-collision groups; split graph: steps inside them. The bank already rejects q-only at both centres (D3). | Ready; C.7 A/B/C are already rejected (W3), so the windows serve to map the step structure |
| E9 matrix | k, l = 1..16, k != l: F.TEST both orders, FDIST(1,2k,2l), FDIST(1,2l,2k), F.DIST both, VAR.S | 120 unordered pairs x 8 = 960 (736 without duplicates) | Compositions 2*min(RT,CDF), 2*min(RT,RN(1-RT)), two-stage, reversed RT. Every S_k has variance 1, so every call is an exact tie: E9 maps the tie convention only. On the 20 tie rows already banked the public compositions match 9-13 of 20 (section 4 R5). | Ready |
| E9 second part | permutations, power-of-two scaling, translations by 2^20, 2^40, 2^52, less symmetric dyadic arrays | unspecified | Variance staging, ratio staging. | Fill in; dc-6332.13 until the matrix is read |
| E10 identity | a, b in {1,2,3,4,7,8,11,16,31}, x in {1/8,...,7/8}: BETA.DIST(x,a,b,FALSE), BINOM.DIST(a-1,a+b-2,x,FALSE) | 810 | RN((a+b-1)*mass) composition: equality; mismatch rejects only the rounded composition. | Ready |
| E10 width | w = 2^m, m in {-10,0,10}, dyadic z: BETA.DIST(z,a,b,FALSE) and BETA.DIST(w z,a,b,FALSE,0,w); widths 3, 5 | unspecified | Post-exponential division: exact power-of-two rescale. Our code (-LN(w) in the exponent): not exact. | Fill in z and shape lists |
| E10 endpoints | x in {0,1}, a, b in {1/2,1,2}, standard and [-2,6] | 18 + 18 | Measure finite values and error codes. Our code: 0 where a > 1 at 0, #NUM! for a = 1 and a < 1 at 0 (K8). | Ready |
| E10 affine | dyadic intervals, then cancellation cases | unspecified | Staging of X-A, A+z(B-A). | Fill in |
| E11 | N in {64,...,2^30}, K = 3N/8: three 3/8 identities; odd N; short support n in {2,8,31}; n = K = N/2 | 21 + unspecified | Exactly 0x3fd8000000000000 under a rational shortcut; other values separate mass graphs. | dc-6332.13. Our cumulative loop would need about 4*10^8 mass evaluations for one call at N = 2^30 (section 6). |

Amended first request (dc-6332.8): E1 (1,052), E2 window (257), E3 (15, plus a general-arm pair set),
E6 bridge (24), E7 ladder + floor + 0.85 neighbourhood + sharp controls (about 3,700), E8 (2,791), E9
matrix (960), E10 identity + endpoints (846); E5 Loader windows (5,654) if size allows. About 9,700 calls
without E5.

---

## 4. Replayable candidates (no new Excel answers), by bank rows reachable

Reach counts are current mismatches of the arms the candidate touches (prompt A.2, A.3, A.5 tables).

**R1. Loader beta prefactor (C.2), dc-6332.4.** `M(a,b,x,y)` = the exact `binom_mass` sequence with
x -> a successes, n = R(a+b), p -> x, q -> y (discrete_dist.rs:93-132); then
`C_mass_ratio = R(R(b/n)*M)`, `C_mass_scale = R(R(b*M)/n)`, and the absorbed form (normalizer
`0.5 LN a - 0.5 LN b + 0.5 LN n + ln sqrt(2 pi)` as running terms). Substitute where the TOMS kernel
uses a prefactor: brcomp/brcmp1 take H = a*C; bpser takes P0 = C/y^b, either `R(C/EXP(R(b*LN y)))` or
with `-b*LN(y)` appended to the running exponent. Use the measured `stirling_error` (non-whole shapes
via [6,7) and the step recurrence), never the C.2 shift-to-7 helper first. Reach: BETA.DIST about 950
(bpser/bup/bfrac arms), FDIST 3,738, F.DIST 189, TDIST 64, NEGBINOM 60, BETADIST about 150, F.TEST 52,
BINOM 7. Rows predicted: not stated by the answer; to be measured. Caveat: arms with min(a,b) < 1 need
s(b) for b < 1, where the helper is unmeasured (GAMMA.DIST pinned it from 1.1).

**R2. bgrat substitutions (C.4), dc-6332.4.** One at a time with the expansion fixed:
(1) Q(b,z) from `regularized_gamma_q` at the same rounded z; (2) r = R(z*gamma density) or
r = b*poisson_mass(b, z) (for b <= 1 this is the GAMMALN route, discrete_dist.rs `poisson_mass`,
x <= 1 branch); (3) the g program of C.4 with l2 = LN(R(a/nu)) and the alternative
l2 = -log1p(R(R(b-1)/R(2a))). Reach: S:bgrat swap 868, S:bup20+bgrat 42, S:bgrat 1, L:bup_bgrat 19,
FDIST L:bup_bgrat 238, F.DIST L:bup_bgrat swap 13: about 1,180. Also re-check the 8 rows lost at
dc-ymjf.6 under each substitution (the answer: a tolerance match that disappears when the prefactor is
corrected is cancellation).

**R3. TINV through the scalar beta coordinate (C.6 t_C), dc-6332.4. Decided on the bank here.**
`TINV(p,nu) = SQRT(R(nu*R(R(1/z)-1)))` with z = the beta inverse at (nu/2, 1/2), for df >= 2 and
p <= 0.85; keep the current solve for p near 1 until the route change is located (E7 ladder). Image test
(tinv_image.py, tinv_density.py, tinv_z.py): t_C holds on 21/21 such rows (t_A/t_B fail on 13); image
density at p = 0.85 is at most 1%, so chance agreement is ruled out; Excel's z lies 0.06-1.9 z-ulps from
the true root. With z = RN(true root) the composition matches 16 of 21 rows (gains oc10, oc11, oc13,
oc14, oc15, oc22, oc23, oc30, oc31; keeps the 7 exact rows), so TINV 15/32 -> 24/32 in that ideal; with
production `beta_inv_kernel` the gain depends on how often it lands on the preimage z. The preimage z
per row (tinv_z.py output) is a ready target list for the BETAINV solver research. The three p = 0.97
rows stay open (no scalar-z transform; small-coordinate preimages 4-11 ulps from the root at df 4 and 25).

**R4. F-native Loader deviance (C.5), dc-6332.4.** `n = R(a+b); r = R(b/n); u = R(f-1);
vlog = log1p(R(r*u)); D_beta = R(R(n*vlog)-R(b*LN f))` in place of the two deviances, keeping the
Stirling and normalizer sequence. It uses f, so the collision screen cannot reject it. Reach: FDIST
3,738, F.DIST 189, F.TEST 52. Run after R1 exists (it is an R1 variant).

**R5. F.TEST tie orientation, dc-6332.4.** On exactly equal variances, take the second array's
degrees as numerator (`var1 > var2` instead of `>=` at statistical_tests_family.rs:60). On the 20 banked
tie rows, approximated from our FDIST bits (ftest_tie2.py; the complement-side rows use `0.5+(0.5-p)`
for bratio's second output): exact 8 -> 11, gains ho-002, -006, -008, -012, -015, -017, losses ho-009,
-016, -018. Excel's own public compositions match only 9-13 of the 20, and F.TEST on a tie depends on
array order (ho-014 0x...318, ho-015 0x...312), so the public composition family is already rejected
there. Small, cheap; measure with the real kernel.

**R6. HYPGEOM (C.7), dc-6332.4, low.** (a) Mass with q = R(1-R(n/N)) instead of R(R(N-n)/N): touches
410 rows, 49 of the 89 mismatches; risky. (b) Orientation swap using R(K/N). (c) Ascending and
descending recurrences for the cumulative (35 cumulative mismatches).

**R7. F state collision screen (E2 offline), done here (f_collide.py, f_collide2.py).** Over the 15
windows (3,855 rows): (v,w) 161 conflicting groups of 467; (v,w,LN f) 2 of 42; (v,w,LN N) 93 of 380;
(v,w,LN D) 136 of 445; (v,w,lv,lw) 76 of 365; (v,w,lv,lw_origf) 74 of 282; (v,w,D) 89 of 412;
LN f alone 64 of 142; d2/N alone 62 of 500. Consistent: (RN(N/d2),RN(d2/N)) 0 of 301; N alone 0 of 272;
(v,w,N) 0 of 272. So a kernel fed from N (or N/d2 with d2/N) survives; every log-prefactor spelling of
C.5 is rejected. Same result with correctly rounded and UCRT logs.

**R8. Unit-shape F rows (f_unit.py, f_unit2.py, f_unit3.py), done here.** On the 47 FDIST/F.DIST rows that
reach the b = 1 or a = 1 arms: LN(RN(v)) (current) 35/47; -LOG1P(N/d2) 21; -LN(1+N/d2) 24; -LN(D/d2) 22.
On 29 FDIST two-arm rows: current 21, 64-bit quotients 12-14, exact quotient 14; separated logs 9/41 on
FDIST, 1/18 on F.DIST. So the unit arms consume the rounded binary64 quotient; any extra information
Excel keeps reaches only the non-unit arms.

**R9. Rejected candidates, replayed here (no further action).** C.7 BINOM A, B, C, POWER variant and the
sum of masses: none matches any of the 6 k = 1 rows (binom_k1.py). C.5 TDIST df = 2: Q_A 0/6, Q_B 3/6,
Q_C 0/6 (current arms 4/6); df = 1 arctan spellings: exactly 0.5 at t = 1 (tdist_low.py). C.6 FINV:
every fixed transform spelling misses 13-21 of 32 rows, and preimage coordinates sit up to 9 ulps from
the true roots, so FINV is not a scalar coordinate with a near-exact inverse (finv_z.py). C.6 d1 = 2
EXPM1 inverse: 2/8 against 3/8 now (finv_d2.py).

---

## 5. Where the answer is wrong, overstated, inconsistent, or in conflict

**W1. The first-ranked F direction, in the spellings C.5 gives, is already rejected.** The separated-log
state conflicts in 76 window groups and the original-f state in 74 (R7); on the unit-shape rows they
score 9/41 and 1/18 against 33/41 and 14/18 (R8). The answer did propose screening (E2), but C.5 presents
these as "fully defining the new wrapper arithmetic". What survives is a state carrying N.

**W2. Wider normalized arguments (rank 3) lose on the unit-shape rows** (12-14/29 against 21/29), in line
with R43's finding that x87 stores in bpser change nothing.

**W3. C.7's BINOM k = 1 spellings are overstated as "a concrete EXP/LN spelling".** A and B are 518 ulps
below Excel at n = 1000 and 21-83 below at n = 100; C is 31-44 off. They reproduce the one-ulp step
shape (any form with original p in the coefficient does) but not the level. At |exponent| about 693,
one exponent ulp moves the result about 1,000 ulps, so EXP(binary64 exponent) times a small exact
coefficient can match only by luck; Excel's level lies 0.034 exponent-ulp from RN(1000*LN 0.5)
(binom_probe.py), which points to a q^n-first form with an unidentified final stage. Also the answer
cites one q-collision witness; the bank has a stronger one (D3).

**W4. The TDIST closed forms do not survive the bank** (R9), and A.3's "even more specific explanation"
of t = 4.2 vs 7.5 is what our code already does; the prompt should have said so (both rows exact).

**W5. A.4/E7 treat the TINV architecture as open; the bank decides it for p <= 0.85** (R3). The answer
was right in substance (D.3), but the recommended near-one batch spends 264 calls on a question whose
p = 0.97 answer is already in the bank (no scalar-z transform there). The prompt's reading of a 1e-15
absolute stopping rule is wrong for p <= 0.85: Excel's z is at or next to the correctly rounded root.

**W6. E3 is overstated ("identifies the graph directly").** Both its calls route to the unit arms
(special_math_common.rs:2323-2330), where the two-stage complement is already established (89/89).
The bank's 76 complementary pairs (complement_pairs.py) give: both graphs 67, two-stage only 4
((1.5,1.5), (8,8), (0.2,50) twice), `1-w` only 0, neither 5 (independent arms, e.g. (0.0625,1,10)
crosses the a = 1 x = 0.1 switch). The prompt counted 71 pairs; the trace gives 76 including one
self-pair.

**W7. E4 largely repeats R44/R45.** With x = 2^-e every LN is distinct, so no L collisions arise without
the windows mentioned in passing; R45 already measured 210 consistent LN classes and 105/216
inconsistent product-prefix classes, rejecting product-only interfaces. Only the product-residual arm
is new.

**W8. E9 is entirely exact ties.** Every S_k has variance 1, so the matrix maps the tie convention, not
the larger-variance rule. That is still useful (R5), but it should be described as such.

**W9. Fractional Stirling helper: conflict with our code and with answer 9.** A.1 says the integer
evidence does not identify a fractional helper and C.2 offers a shift-to->=7 candidate. Our code has a
measured real-shape helper that applies the series in [6,7) and steps down (discrete_dist.rs:61-77),
whose series truncation at 6 is deliberately reproduced ("puts the error of -3.6 units of 2^-53 just
above each integer"). Answer 9 (A.3, "the recurrence-from-6 correction has a substantive truncation
problem") puts that truncation near u = 6 at 4.1e-16 to 6.1e-16 and against 2.4e-17 at 7: the C.2
candidate differs from the measured helper by exactly that. Use the measured helper for C.2; answer 9's
point concerns the GAMMA seed, not this helper's measured use in GAMMA.DIST.

**W10. "Already successful gamma-family upper-tail evaluator" (A.1, C.4) is only partly measured for
b < 1.** In the bank: GAMMA.DIST cumulative with shape < 1: 193/204; CHIDIST df = 1 (shape 1/2): 181/181;
other shapes below one in the upper tail are not covered. Answer 9 (A.4, "The small-shape normalization
is unresolved") says the a < 1 normalization is open. And substitution 2 for b <= 1 runs through
`poisson_mass`'s GAMMALN route (x <= 1 branch), a public-GAMMALN prefactor the answer itself ranks low
in A.1. The substitution is still the right experiment; its expected yield is lower than D.2 suggests.

**W11. A.2 understates the N result.** "Zero conflicts for RN(d1*f) ... happens to preserve enough
information" suggests a vacuous test; in fact N collides in 272 window groups and every one is
consistent. It is a real survivor, as are (RN(N/d2),RN(d2/N)) and (v,w,N).

**W12. C.7's HYPGEOM mass is not the established graph.** It keeps the (a/c)*b order but forms
q = R(1-r); the code forms q = R(R(N-n)/N) (K16), and the two differ on 410 of 973 rows.

**W13. Internal tension on BINOM.** A.5 and D.1 rightly say retaining p and q is not enough (R46:
6/774), while C.7 specifies "retain original p together with q" as the general BINOM route and offers
only k = 1 spellings; for general k the answer gives no executable program.

**Conflicts with earlier rounds.**
- W109 (code note, chi_f_t_family.rs:317-320): TINV = SQRT(df*(1/BETA.INV(p,df/2,0.5)-1)) on 135/135.
  The answer re-derives it. The bank agrees for p <= 0.85 but rejects every scalar-z form at p = 0.97,
  so W109's 135 calls cannot have covered p near 1 for these df (or a build differs). Record this in
  dc-6332.4.
- R45: see W7. R46 and R20: consistent with the answer's caution; C.7's k = 1 spellings join the
  refuted list (W3). R43: consistent with W2. R33: FDIST = F.DIST.RT kept.

**Conflicts with answer 9 on shared code** (incomplete gamma, Loader, Poisson).
- Stirling helper: W9. Small-shape gamma normalization: W10.
- Deviance threshold: both keep the measured 0.2 (answer 10 explicitly, against Loader's 0.1). No
  conflict.
- Answer 9 C.6 (one-boundary large-Poisson store change) acts on the large-mean upper-tail route for
  integer shapes; bgrat's Q(b,z) has b <= 1, so the overlap is small. Any dc-6332.3 change to
  `stirling_error`, `deviance`, `poisson_mass` or `regularized_gamma_q` changes R1/R2 baselines: land or
  park dc-6332.3's C.5/C.6 before scoring R2, or re-score after.
- Answer 9's warning against obtaining P by subtraction from Q agrees with answer 10's "do not
  substitute R(1-GAMMA.DIST)". No conflict.

---

## 6. Proposed new bead

**HYPGEOM.DIST cumulative: bound the per-call work at large populations.** The cumulative sums masses
one by one (discrete_dist.rs, the loop after the `s > n.min(k)` test), each mass three `binom_mass` calls
on the x87-emulated chain. E11's HYPGEOM(K-1,N-1,K,N,TRUE) at N = 2^30 means about 4*10^8 iterations in
one cell, so the local replay of E11 is impractical and a real workbook can stall a recalculation. First
measure the time of one call at N = 10^4, 10^6, 10^8; then, if needed, a recurrence or complement route
that keeps today's bits on the banked small-N rows (1,450/1,539), checked against E11's answers when
dc-6332.13 schedules them. This is a product issue outside the three pro-9 beads.

---

## 7. Top five actions by expected value

1. **dc-6332.4: Loader beta prefactor (R1)**, with the measured real-shape Stirling helper and the
   F-native variant (R4) as one family. Largest reach (about 6,000 mismatching rows across BETA.DIST,
   FDIST, F.DIST, TDIST, NEGBINOM, F.TEST).
2. **dc-6332.4: the three bgrat substitutions one at a time (R2)**, about 1,180 rows; score after
   dc-6332.3's shared-helper changes are settled; expect less than D.2 claims for b < 1 (W10).
3. **dc-6332.4: TINV = SQRT(nu*(1/z-1)) on the scalar beta coordinate for p <= 0.85 (R3)**, using the
   preimage z list as the target for the BETAINV inverse; up to +9 TINV rows with an exact z, and the
   cheapest way to sharpen BETAINV.
4. **dc-6332.8: the amended first request (section 3)**: E1, E2 window, E6 bridge, E8, E9 matrix, E10
   identity and endpoints, the 0.85 neighbourhood and sharp controls, and a TINV p ladder between 0.85
   and 0.97 in place of most of E7's near-one batch (about 9,700 calls, E5 Loader windows if room).
5. **dc-6332.4: record the rejections (R7-R9, W1-W4) in the bead and replay R5 (F.TEST tie
   orientation)**, so the replay lane spends nothing on the C.5 log states, the TDIST closed forms, the
   BINOM k = 1 spellings or fixed FINV transforms, and takes the cheap +3-row tie change if the real
   kernel confirms it.
