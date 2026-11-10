# Harvest of answer 9: gamma family (GAMMA, GAMMALN, incomplete gamma, inverses)

Reader: Opus, 2026-11-10, for dc-6332.1. Answer: `pro/9-gamma-family-answer.md`; prompt: `pro/9-gamma-family-prompt.md`,
pinned to b2a80850 (no code differs between b2a80850 and the current master; all `file:line` below hold for both).
All paths are under `crates/calc-func/src/ported/` unless stated.

How the checks were done: reading the code; exact-rational Python for every bit deduction (x87 `RN64`/`RN53`
rounding emulated with fractions); the witness traces `codex-runs/pro-9/v2/trace-<FUNCTION>` for bank rows; the
history reports `codex-runs/pro-9/history/work-report-<n>.md`; and the R35 evidence in
`C:\Work\DnaCalc\DnaCalc-Research\evidence\round35`. Scratch scripts: `C:\Work\DnaCalc\codex-runs\pro-9\work\harvest-9\c1..c19*.py`.
Where Excel's own EXP, LN or ERFC was needed I used a correctly rounded value as a proxy. Each such result is
labelled "(proxy)". The EXP proxy reproduced our current output on 101 of 101 integer-shape rows and 29 of 30
half-integer rows, so it is sound on those rows. The CR erfc proxy is not: it equals Excel's ERFC.PRECISE on only
61 of 181 CHIDIST df=1 rows. No cargo run, no Excel, no tracker writes.

Five findings change how the answer should be used:

1. **The answer's sqrt(pi) audit (A.4, C.1, C.5) is wrong.** RN53 of the mathematical sqrt(pi) is
   `0x3ffc5bf891b4ef6b` (6b). The landed literal `1.772_453_850_905_516` (`special_math_common.rs:84`) also parses to 6b.
   So there is one constant, not two. The decimal `1.7724538509055159` that the prompt prints for 6b actually parses to
   6a, and the answer's audit was built on that misprint. `RN53(sqrt(RN53(pi)))` is 6a, not 6b. The midpoint of 6b
   and 6c is about 1.77245385090551621, not ...599. C.5's `C = 0x3ffc5bf891b4ef6c` is therefore not the landed
   constant.
2. **The stored-6a GAMMA seed (A.3, C.3, C.8) is half right.** It does fix GAMMA(-0.5) and GAMMA(-1.5), and the
   identity 2M = 3N+1 holds. But it breaks GAMMA(-2.5), which matches today (Excel `0xbfee3ff812e32183`; the seed gives
   ...182). No 64-bit resident seed fits all three rows under the coded below-one chain. So the bank already refutes
   the common-seed graph: net +1 row if landed, and as a special case it would amount to a hard-coded answer.
3. **Two B3 values decide 32 of the 58 GAMMA.DIST cumulative misses.** The a = 1.75, 2.75 and 5.5 misses track
   Excel's own GAMMALN(2.75), GAMMALN(3.75) and GAMMALN(6.5), which are in the bank. Ours is +4, +1 and +1 ulp there.
   The relative shift we predict from that leaf (-2, -2 and -8 units of 2^-53) matches every one of the 32 rows. The
   (3,8) recurrence reduces GAMMALN(3.75) and GAMMALN(6.5) to B3(2.75) and B3(2.5) (proxy: with Excel's B3 values the
   recurrence gives Excel's bits). B3(2.5) is itself 1 ulp off. R33 had already run this leaf-injection diagnostic on
   the density (108/108).
4. **The R35 integer-shape polynomial (D.8 step 4) is the best replay, and it exists as a patch.** The patch is
   `DnaCalc-Research/evidence/round35/retained/gamma-forward-parked.patch.gz`. It matches 101 of 101 bank rows in its
   dispatch (proxy), where ours matches 90: +3 GAMMA.DIST, +3 POISSON.DIST, +5 CHIDIST, no forward losses. R35 also
   already separated H64 from H53, the two variants B.6 asks to compare.
5. **C.3's x >= 10 spec does not describe our code.** It uses the fully stored GAMMALN correction S10
   (`x87_chains.rs:197-206`). Our GAMMA x >= 10 (`gammaln.rs:395`) uses `excel_stirling_series`
   (`x87_chains.rs:166-195`): t = R64(1/R64(n*n)), Horner in the registers, one store of R64(w/n). The C.3
   "nine exceptions" candidate has to be restated against that graph.

---

## 1. Disposition of every section

| Section | Content | Disposition |
|---|---|---|
| A.1 table | Component-by-component shape and confidence | no action: summary of A.2-A.5 (each is placed below) |
| A.2 | GAMMALN root-preserving rationals; LN-of-stored-Gamma excluded; B3 graph kept; three qualifications | **dc-6332.7** (B.2, B.3 are its tests). The text is method: no action beyond that |
| A.3 | GAMMA seed: sqrt(2pi) normalisation, ranking of seed families, hidden-half deduction, below-one contradiction, recurrence-from-6 truncation, reflection, tiny plateau | Hidden-half deduction: **dc-6332.7** (B.4); see section 4 R5 for why not a replay. Seed-family ranking: **dc-6332.13** (D.8 step 7). Truncation argument: no action; it agrees with our own code comment (`discrete_dist.rs:67`) |
| A.4 lower series, four upper-tail questions | Keep `P = st(M*A)`; separate the normalisation, store, tail and P/Q publication | no action: describes the current code (`special_math_common.rs:635-690`) |
| A.4 small-shape normalisation | a < 1: shared mass versus separate reciprocal-Gamma correction | **dc-6332.7** (B.1) |
| A.4 integer finite polynomial | R35 graph, narrow scope | **dc-6332.3** (replay R1) |
| A.4 half-integer sums and the two sqrt(pi) constants | Keep the landed literal; two constants | **dc-6332.3** (replay R4, with the constant corrected to 6b); the constant audit itself: no action, it is wrong (section 5) |
| A.4 two corrections | Density uses the original x (a/x); misses at a >= 2 also take the GAMMALN route; scale-one concentration is unpaired | no action: all three are confirmed (section 2). Fix the prompt-era docstring at the next touch of `discrete_dist.rs:138` |
| A.4 large-mean Poisson | E-sensitivity argument; resident-prefactor hypothesis | **dc-6332.3** (C.6 replay R3) and **dc-6332.7** (B.8) |
| A.5 plateau analysis | CHIINV(.99,2) plateau about 64.6 ulp wide; ours at the low edge | **dc-6332.13** (inverses), with the bank facts in section 4 R7 |
| A.5 method ranking | Newton > secant > Halley > bisection hybrid | **dc-6332.13** |
| A.5 documentation table | 64 / 64 / 100 / 64 / none | no action. R5 checked: the CHIINV page says 100 iterations. R3, R4, R6, R7 not re-fetched |
| B contract, W(b) | Window definition, bit injection | no action: dc-6332.7 adopts it as its protocol |
| B ranking table | Alternatives per region mapped to B.1-B.10 | no action: index of B.1-B.10 |
| B.1 | Density-to-CDF exact prefactor probe; threshold 1.1; scale repeats | **dc-6332.7** (first) |
| B.2 | Power-of-two root offsets for B1 and B2 | **dc-6332.7** |
| B.3 | B3 equal-spacing grid across 2, stage-crossing windows | **dc-6332.7** (high value: section 4 R2) |
| B.4 | Exact-translation seed lattice, hidden-half windows, tiny plateau | **dc-6332.7** |
| B.5 | Seed families by leaves; EXP(GAMMALN) composition; Lanczos tracks; D(t) | **dc-6332.7** for the EXP(GAMMALN) and LN-threshold windows; **dc-6332.13** for the Lanczos-candidate tracks. D(t): no action, the code comment at `discrete_dist.rs:58-63` says it was already used (2,430 shapes) |
| B.6 | Integer-polynomial dispatch | **dc-6332.3** (land R35 first); the remaining edges (a = y, shape 20 and 21, 30 and 31) go to **dc-6332.7**. H53 versus H64: no action, R35 settled it |
| B.7 | Half-integer first term; general CF stop | **dc-6332.7**, after replay R4 |
| B.8 | Large-tail boundary and publication; k=0 mean 60.0147 anomaly | **dc-6332.7**, after replay R3 |
| B.9 | Inverse plateaus on elementary forwards; complement identities | **dc-6332.13** (inverses deferred), with the bank evidence in R7 attached |
| B.10 | Shape-20 forward versus inverse | **dc-6332.13**; the forward half (Temme at a=20, p=0.07) can go in dc-6332.7 when its first batch runs |
| C.1 | Arithmetic and constants table | no action, except that the two sqrt(pi) rows are wrong (section 5) |
| C.2 | GAMMALN shell and provisional coefficients | no action: it transcribes the current code (check in section 2); B1 and B2 recovery goes to dc-6332.7 |
| C.3 below 10 | Generic product and below-one graph | no action (it is the current code) |
| C.3 x >= 10 | Stirling product spec, plus "retain R64(z*w)" for the nine exceptions | Restate against `excel_stirling_series`, then **dc-6332.3** as a low-priority replay (R6) |
| C.3 reflection and sine lattice | Common-sine preimage test | **dc-6332.7** |
| C.4 | Diagnostic recurrence-from-6 seed | **dc-6332.13** (diagnostic; the answer expects no gain) |
| C.5 lower series, H53/H64, T53/D53 | Spec | R35 = H64: **dc-6332.3** (R1). T53/D53: **dc-6332.3** (R4) with C = 6b |
| C.6 | One-boundary large-Poisson candidate | **dc-6332.3** (R3) |
| C.7 | Newton and Halley probes | **dc-6332.13** |
| C.8 table | Predicted row changes | Checked row by row in section 4. GAMMA(-1.5)/(-0.5): **dc-6332.7**. GAMMALN rows depend on unrecovered tables: **dc-6332.7** |
| C.8 Appendix F paragraph | Exact leaves fix the a = 1.75, 2.75, 5.5 chains | Confirmed on the bank (R2 below); it becomes the acceptance target for **dc-6332.7** B.3 |
| D.1 | Rounding preimages as constraints | **dc-6332.13** (research briefs); used here in section 4 R5 |
| D.2 | Signed stage gains | **dc-6332.13** |
| D.3 | Power-of-two coefficient scale is unidentifiable | **dc-6332.13**; note it in the B3 brief for dc-6332.7 |
| D.4 | Tables typed as separate literals | **dc-6332.13** |
| D.5 | Seams versus dispatches | **dc-6332.13** |
| D.6 | Inverse as a second oracle | **dc-6332.13** |
| D.7 | Evidence ledger | no action: this harvest is that ledger (section 2) |
| D.8 | Run order | Steps 1-6: **dc-6332.7**, except steps 1 (local half) and 4, which go to **dc-6332.3**. Step 1's local half was already done in R33 and is confirmed here. Step 7: **dc-6332.13** |
| References | R1-R8 | no action. R8 (Boost sqrt(pi)) was misapplied (section 5) |

No new bead is needed. Everything fits dc-6332.3, .7 or .13.

---

## 2. Claims about our code and the prompt's data

| # | Claim (answer section) | Verdict | Evidence |
|---|---|---|---|
| 1 | x-1 on [0.7,1.5) and x-2 on [1.5,3] are exact subtractions (A.2) | agrees | Sterbenz: 0.5 <= x <= 2 and 1 <= x <= 4 |
| 2 | LN of a stored Gamma near 1 would be 10^14-10^15 ulps wrong at x=1+2^-52 (A.2) | agrees | Lattice step 2^-53 against the result ulp 2^-105: about 7e14 |
| 3 | C.2 B3 graph, coefficients and x=3 pin | agrees | `gammaln.rs:162-201`; the decimals equal the code bits for all 18 coefficients (checked) |
| 4 | C.2 S10 coefficients = RN53 of the exact quotients, stored program | agrees | `x87_chains.rs:170-181, 197-206`, all 10 checked |
| 5 | C.2 x >= 8: x-0.5 kept "inside the register expression" | partly agrees | `gammaln.rs:145` subtracts in binary64 before the x87 multiply. Identical for x < 2^52; differs above |
| 6 | C.2 x < 0.7: plain-double final subtraction | agrees | `gammaln.rs:295` |
| 7 | C.2 "+0 at x=1" | partly agrees | Published bits are +0 (trace); internally `gammaln_excel(1.0)` is -0.0, asserted at `gammaln.rs:533-540` |
| 8 | C.3 generic 1<x<10 and below-one graphs | agrees | `gammaln.rs:422-466`; x+k in binary64 and R64 divisions. Note: y = x - floor(x) is inexact for x in (-0.5, 0) |
| 9 | C.3 x >= 10 ends with `EXP(S10(x))`, S10 the fully stored program | **disagrees** | `gammaln.rs:395` calls `excel_stirling_series` (`x87_chains.rs:166-195`): registers, one store of R64(w/n). The prompt's G.5 wording ("stored once") was right; the answer conflated it with GAMMALN's S10 |
| 10 | C.3 reflection graph | agrees | `gammaln.rs:403-420` (fold, stored pi*t, CRT sine, R64 product and divide, sign by parity) |
| 11 | Positive half-integer GAMMA uses 6b; GAMMA(-0.5) goes through the generic route (A.3) | agrees | `special_dist_family.rs:137-145` covers x > 0 only. Our Cody seed(0.5) rounds to 6b (Python, plain double) |
| 12 | Hidden-half deduction RN53(S) = `0x3fec5bf891b4ef6a`; GAMMA(-1.5) ends f1; 2M = 3N+1 (A.3, B.4, C.8) | agrees, but does not generalise | `c2_gamma_halves.py`: the seed gives -0.5 → ...6a and -1.5 → ...f1 (both Excel). It gives -2.5 → ...182, but Excel is ...183, which ours matches today. `c3_resident_seed.py`: -1.5 needs a resident offset <= 1/4 ulp, -2.5 needs >= 0.344 ulp, so no common seed exists |
| 13 | sqrt(pi): 6b is not RN53(sqrt(pi)); the landed literal and the mathematical value round to 6c; 6b is sqrt(double pi) (A.4, C.1, C.5) | **disagrees** | `c1_constants.py`: RN53(sqrt(pi)) = 6b; `1.772453850905516` → 6b; `1.7724538509055159` → 6a; RN53(sqrt(RN53(pi))) = 6a; RN53(sqrt(RN64(pi))) = 6b |
| 14 | The density uses st(a/x) with the original x; G.9's "a/y" is wrong (A.4) | agrees | `beta_gamma_stats_family.rs:139` uses `excel_x87_div(alpha, x)`. The docstring at `discrete_dist.rs:138` still says a/y |
| 15 | The 99 GAMMALN-route density misses are not all at a < 2 (A.4) | agrees | Trace: 73 at a < 2, 26 at a >= 2 (all beta=1). Loader route: 10 at 1<a<2 |
| 16 | Leaf for shape a is GAMMALN(a+1); 2.75 → B3 by recurrence; 5.5 → B3(2.5); effect up to a < 7 (A.4) | agrees | `discrete_dist.rs:155-157`, `gammaln.rs:253-267, 286-301`. Confirmed numerically in section 4 R2 |
| 17 | The 4,000 beta≠1 density matches do not establish a scale-one defect (A.4) | agrees, stronger | Every GAMMALN-route density row at beta≠1 has integer shape 1, 2, 3, 5 or 8 (1,973 rows); 0 of the 99 failing shapes were ever sampled at beta≠1 (`c8_scale_pairing.py`) |
| 18 | Large-mean Q runs through Loader V = EXP(E), D = sqrt(2 pi a), Q = st(st(M*st(a/y))*(1+sum)) (A.4, C.6) | agrees | `discrete_dist.rs:159-163`; `special_math_common.rs:695`. For a >= 20 (k = 19, 29, 53, 89), statement 30 falls through to the same tail (`special_math_common.rs:594-620`) |
| 19 | EXP(-mean) rounds to 0 at 796, 909, 1010; Q/V about sqrt(2 pi a) y/a; ulp(E) = 2^-43 for E in [512,1024) (A.4) | agrees | Arithmetic: E ≈ -701 at k=1, mean 715; the factor is about 1,266 there |
| 20 | Our inverse publishes the first x with Q(x) <= p, the low edge of the plateau (A.5) | agrees | `chi_f_t_family.rs:139`, `special_math_common.rs:2511-2534` |
| 21 | CHIINV(.99,2) plateau ≈ 64.6 root ulps; ours 31.8 below truth (A.5) | agrees | `c1_constants.py`: width 64.65 |
| 22 | The three scales of the a=20, p=0.07 quantile are one observation; unit root 0x402bac06c70d46b8 (A.5, B.10) | agrees | Scales 0.5 and 2 rescale exactly to the same unit bits. All 148 unit keys are consistent across scales |
| 23 | Next Stirling terms T11 = 854513/(63756 u^21), T12 = -236364091/(1506960 u^23); remainder 4.1e-16 to 6.1e-16 at u=6, 2.4e-17 at 7 (A.3) | agrees | True remainder 4.66e-16 = 4.20 units at u=6 and 0.18 at u=7. Our own comment (`discrete_dist.rs:67`) records -3.6 units just above integers in the Loader masses |
| 24 | Binade facts for g=7 and g=607/128 (B.5) | agrees | T = t+6.5 crosses 8 at t=1.5; LN in [2,4). T = t+4.24 stays in [4,8) |
| 25 | All hex inputs in B.1-B.4 and B.9 | agree | 0.75, 1.75, 2.75, 5.5, 1.625, 2.5, 0.7, 1.5, 3, 8, RN53(1e-16), 2^-53, 2^-52, RN53(1.1), 0.99 = 0x3fefae147ae147ae, all checked |
| 26 | B.2 rescaled ratios 0xbfe2788cfc6fb61a and 0x3fdb0ee6072093cc | agrees | Exact scaling. The second is D2 (`gammaln.rs:18`) minus 2 ulp |
| 27 | Appendix C's high/low split for the nine x >= 10 misses differs from its histogram (D.7) | agrees | Trace: 7 high, 2 low; the prose says 5/4 |
| 28 | The half-shape Q0 is "the established public half-shape ERFC path, a fixed leaf" (A.4, C.5) | **disagrees** | `special_math_common.rs:567`: `summ = gratio(0.5, x).1`, i.e. the a<1 continued fraction with the NSWC `gam1` prefactor for x >= 1.1 (`:524-530`). That is not `erfc_precise_kernel`. See R4 |
| 29 | C.5 T53 is the current graph | agrees in structure; **constant wrong** | `special_math_common.rs:566-575` = T53, but with C = 6b (`:84`), not 6c |
| 30 | Lower series and companion publication (C.5) | agrees | `special_math_common.rs:676-689` |
| 31 | H53 versus H64 is open (C.5, B.6) | **disagrees** (already settled) | R35 `integer-gamma-coefficients-fresh.json`: stored_pc64 2523/2523, strict 2519/2523, resident 1035. Shape 20: 261, 260, 63. The archived program is H64 |
| 32 | C.6 graph pieces exist as stated | agrees | E, V, D and u as above; A = binary64 `1.0 + summ` |
| 33 | The C.7 Halley formula | agrees | delta_H = delta/(1 - delta*h/2), with h = (a-1)/x - 1 = f''/f' for both tails |
| 34 | The positive half-integer path "multiplies by RN(sqrt(pi))" (prompt G.4, `special_dist_family.rs:135`) | agrees (the answer said no) | 6b is RN53(sqrt(pi)) |

---

## 3. Excel experiments (all for dc-6332.7 unless marked .13)

Call counts assume W = 1,024 inputs and count one Excel cell per function per input.

| Exp | Inputs (as the answer gives them) | Calls | Predicted outcome per hypothesis | Ready? |
|---|---|---|---|---|
| B.1a | Shapes in W(0x3fe8…), W(0x3ffc…), W(0x4006…), W(0x4016…) (0.75, 1.75, 2.75, 5.5); x = ldexp(a,e), e = -80, -96, -112; b=1; GAMMA.DIST TRUE and FALSE | 4×1024×3×2 = 24,576 | Shared stored mass: C == ldexp(D, e) bit for bit (for a >= 1 our own code makes this an identity). Separate a<1 normaliser: a ≈ 0.75 may differ; a discrepancy constant across e points to the normaliser, one tied to rounding phase points to a resident intermediate | Ready. Bank proxy (`c19`): shared mass with Excel's own GAMMALN(1.75) gives 6/20 on the a=0.75 cumulative rows, against ours 9/20. That is weak evidence for a separate normaliser, consistent with R33's 0.75/0.375 note |
| B.1b | Mean window W(0x3ff199999999999a) at a=2.75 and 5.5, density and CDF | 1024×2×2 = 4,096 | GAMMALN leaf the cause: current-minus-Excel negative below 1.1, collapsing above | Ready. Already explained from the bank (R2) |
| B.1c | Each failing unit-scale density row at b = 1/2, 2, 4 with x = b·y | 109×3 = 327 | Normal intermediates: exact power-of-two scaling, so the same failures reappear | Ready |
| B.2 | GAMMALN at 1±2^-k (k=2..52 up, 2..53 down) and 2±2^-k (k=1..51 up, 1..52 down) | 206 (×2 aliases optional) | Factored candidate reproduces the rescaled ratio bits; LN-of-stored cannot; cancellation form shows an absolute floor | Ready to collect. The candidate graphs to score against still need writing |
| B.3a | x_j = 2 + j·2^-51, j = -512..511, j≠0 | 1,023 | A shared table fits one coefficient set across 2; a separate table resets its phases | Ready |
| B.3b | W at 1.625, 2.5, 0.7, 1.5, 3, 8 | 6,144 | Controls; 0.7 and 3 through public recurrences; 8 through Stirling | Ready |
| B.3c | Windows at the crossings \|H_j(x)\| = 2^e of each stored B3 stage on [2,3] | unspecified (≈1,024 per crossing) | Correct coefficients predict crossing location, phase and signed amplitude | **Needs filling**: compute the crossings from `B3_P`/`B3_Q` first. Acceptance target: Excel GAMMALN(2.5) = 0x3fd2383e809a67e8 and GAMMALN(2.75) = 0x3fde69eac73eab04 (R2) |
| B.4a | y_j = 1/4 + j·2^-49, j = 0..1023; GAMMA(n+y_j), n = -9..9 | 19,456 | Common stored seed: equality in every row. Resident: survives more but must round to the public seed. Empty intersection refutes | Ready |
| B.4b | 1,024 consecutive doubles from 0x3fd0000000000000 (GAMMA(y) and GAMMA(1+y)) | 2,048 | Original-y versus rounded 1+y dependence | Ready |
| B.4c | Windows around 1.5, -0.5, -1.5 | 3,072 | Stored generic: -0.5 → 0xc00c…6a, -1.5 → 0x4002…f1 (both already observed) | **Add W(-2.5) and W(-3.5)**: the bank's -2.5 already contradicts this seed (claim 12). Then ready |
| B.4d | W around 0x3c9cd2b297d889bc, 0x3ca0000000000000, 0x3cb0000000000000 | 3,072 | Hard branch: equality through the threshold. Plateau: common-seed cells | Ready |
| B.5a | W around 1.25, 1.5 (override excluded), 1.75: GAMMA(t) against EXP(GAMMALN(t)) | 6,144 | Public composition predicts exact equality | Ready (EXP(GAMMALN) can be one cell or two) |
| B.5b | W at the least t in (1,2) with LN(t) reaching 2^-j, j = 1..4 | 4,096 | Stored LN(t): quantisation change at each | Ready once the 4 thresholds are computed (trivial) |
| B.5c | Lanczos g=7 and g=607/128 phase tracks | 0 new beyond B.4/B.5 | Different phase tracks per candidate | **.13**: needs frozen Lanczos candidates |
| B.5d | D(t) = GAMMA.DIST(t,t,1,FALSE), 1.1 <= t < 2 | unspecified | Exposes EXP(-s(t))/sqrt(2 pi t) | Largely done (`discrete_dist.rs:58-63`: 2,430 shapes); no action |
| B.6 | a = 2..20 windows at y = a, 30, 31; dyadics; shapes 19.x, 20, 20.x, 21 | about 3 aliases × 60 W ≈ 180,000 | Horner table predicts full output in its dispatch | **Trim**: R35 already validated the interior and found the edges (the patch encodes a < x <= 30 with a == x excluded; R37: 25/207 outside). Remaining: shape-20 full-bank effect and a == x. Run after R1 lands |
| B.7a | Means 25/16, 9/4, 49/16, 25/4, 49/4; W at each; CHIDIST(2y,3), CHIDIST(2y,5), ERFC base, GAMMA.DIST | 5×1024×4 = 20,480 | T53 against D53 give exact bits relative to the leaves | **Fix the constant to 6b first**; run after R4 |
| B.7b | a = 2.3, 2.75, 5.3; W at y = a+1/4, a+1, 30.5; GAMMA.DIST TRUE; a=0.75 at W(1.1) | 9,216 + 1,024 | Stop error: changes correlated with the last convergent; persistent phase means recurrence or stores | Ready to collect. No candidate is frozen (the answer admits this); the CF code is at `x87_chains.rs:292-330` |
| B.8a | W at the six (k, mean) pairs listed; three upper-tail aliases | 6×1024×3 = 18,432 | Resident quotient: sign(Q_old - Excel) = sign(st(V/D) - RN64(V/D)), magnitude Q_old - Q_new | Ready after R3 has computed V, D, Q_old, Q_new locally |
| B.8b | Points around 0x0010000000000000 where store-then-flush and test-then-store disagree | small, selected | The two orders differ only where the resident Q rounds up to min normal | **Needs filling**: select the points from the local model |
| B.8c | Mean 60.01467269596327 and W: POISSON(0,m,TRUE/FALSE), EXP(-m), CHIDIST(2m,2) | 4,096 | Shared leaf: four identical patterns | Ready. Our code returns `excel_exp(-mean)` for both k=0 forms (`discrete_dist.rs:332`); the bank has only the TRUE row |
| B.9a | W at the Excel outputs CHIINV(.99,2), CHIINV(.99,1), GAMMA.INV(.99,.5,1), forward functions | about 3,072-6,144 | Public-lattice: the published edge. Smaller tail or resident residual: inside the plateau | **.13**. Ready |
| B.9b | CHIINV(p,2) against st(-2 LN p) | 0 new | Special form: equality | Already answered: 10/10 bank rows equal CR(-2 ln p) (proxy); GAMMA.INV(p,1,1) = CR(-log1p(-p)) on 10/10. Equally consistent with an accurate solver (section 4 R7) |
| B.9c | W(p=0.99) with p >= 1/2: CHIINV(p,k), CHISQ.INV(1-p,k), st(2·GAMMA.INV(1-p,k/2,1)), k = 1, 2, 3, 4, 10, 40, 50; p = 1/2 + j·2^-53 | 2×1024×7×3 = 43,008 | Common canonicalisation: identical bits | **.13**. The third column is redundant (CHISQ.INV(p,2a) = GAMMA.INV(p,a,2) = st(2·unit) was measured 300/300). Bank: the exact-complement pairs (p = 0.5, 0.75) agree 6/6; p < 0.5 pairs are not exact complements |
| B.10 | W(0x402bac06c70d46b8) at shape 20 and adjacent shapes; shape 25 control | about 4,096 | Our Temme forward below target at xE; Excel's forward not | **.13** (forward half could join dc-6332.7) |
| C.3 sine | t = 1/4 + j·2^-45; m = n+t, n in {10, 16, 32, 64, 128, 170}; GAMMA(-m) and GAMMA(m) | 12,288 | A correct graph admits one common sine per t | Ready |
| C.7 | Newton/Halley one-step probes at observed inverse outputs | 2 per row (CDF and density) | Zero-step model: next = x | **.13** |

---

## 4. Directly replayable candidates (no new Excel answers), by rows reachable

**R1. R35 integer-shape polynomial (H64).** dc-6332.3; D.8 step 4, C.5 H64, B.6.
- Computation: for integer 2 <= a <= 20 and a < y <= 30:
  `poly = RN53(1/(a-1)!)`; for j = a-2 down to 0: `poly = st(st(poly*y) + RN53(1/j!))`;
  `Q = st(EXP(-y)*poly)`; `P = 0.5 + (0.5 - Q)`.
  Dispatch it before the a < BIG test, so a = 20 is included; a == y and 30 < y < 31 go to statement 20.
- Source: `DnaCalc-Research/evidence/round35/retained/gamma-forward-parked.patch.gz`, which carries the coefficient
  bits and a test. **Keep `GRATIO_RTPI_FULL`**: the patch predates dc-ymjf.6 and would revert the half-integer
  constant to the 15-digit `GRATIO_RTPI`.
- Bank rows (proxy, `c13_r35_proxy.py`): 101/101 in dispatch against ours 90/101.
  - GAMMA.DIST +3: a=3, y = 3.0000000000000013, 3.875, 5.625.
  - POISSON.DIST +3: (1,5) twice, (4.25,6.125).
  - CHIDIST +5: df 4 at x = 35.395054586014794, 35.3950545860148, 35.39505458601481; df 6 at x=48; df 10 at x=32.
- Inverses: R35's trial at the time measured bank +18/-4 and corpus +6/-0; it was parked for 4 exact inverse pins.
  The current landing rule allows listed losses. Measure GAMMA.INV, CHIINV, CHISQ.INV and the BETA family (answer 10
  C.4 may route `bgrat` through `gratio`).

**R2. B3 at 2.5 and 2.75: acceptance target, not a replay.** It goes to dc-6332.7, with the diagnostic in dc-6332.3.
- Every one of the 32 cumulative misses at a = 1.75 (6), 2.75 (20) and 5.5 (6) has a relative error (ours - Excel)/Excel
  of -1.2 to -3.5 units (pred. -2), -1.3 to -3.2 (pred. -2) and -7.0 to -8.5 (pred. -8). The prediction is the
  difference between our GAMMALN(a+1) and Excel's (`c6_gdist_leaf.py`).
- GAMMALN(6.5) = st(LN(216.5625) + B3(2.5)) and GAMMALN(3.75) = st(LN(2.75) + B3(2.75)) reproduce Excel exactly with
  Excel's B3 values (proxy LN, `c7_leaf_chain.py`).
- So a B3 table that gets x = 2.5 and x = 2.75 right flips up to 32 GAMMA.DIST rows, plus GAMMALN at 2.5, 2.75, 3.75
  and 6.5 (and their recurrence images). It also touches the density misses at shapes whose a+1 reduces to those points.
- Diagnostic (local): inject the bank's GAMMALN at 1.75, 2.75, 3.75 and 6.5 into `poisson_mass` and confirm the 32 flip.
  This is never landable, because it hard-codes answers. R33 did the same on the density (108/108).

**R3. C.6 one-boundary large-Poisson candidate.** dc-6332.3.
- Computation: in the asymptotic tail (`special_math_common.rs:695`) and in `poisson_mass`'s Loader route, keep
  M = R64(V/D) resident. Then `T = st(M*u)` and `Q = st(T*A)`, with E, V, D, u and A unchanged.
- This needs a resident-returning variant of `poisson_mass` for this caller only. The density and the mass keep the
  stored M.
- Reach: of the 542 misses (mean >= 700), at most the 494 with |d| <= 2 can move (|Q_old - Q_new| <= about 2 ulps).
  The 470 near-min-normal misses split -1:179, +1:131, -2:77, +2:49.
- Risk: the 273 matched rows in the same zone (249 near min normal, 24 larger). The sign test is part of the replay:
  each changed row must satisfy sign(Q_old - Q_new) = sign(st(V/D) - RN64(V/D)) automatically. A row is a gain only if
  Q_new == Excel.
- The answer gives no exact row predictions; the replay must produce them. Expected value: it may move nothing.

**R4. Half-integer finite sum: Q0 from the public ERFC.PRECISE leaf.** dc-6332.3. The answer assumed this is already
the code (claim 28).
- Computation (`special_math_common.rs:567`): `summ = erfc_precise_kernel(sqrt(x)) value` in place of
  `gratio(0.5, x).1`. The rest is unchanged: T53 with C = 6b.
- Bank (Excel's own ERFC value from CHIDIST df=1 at 2y, EXP proxy; `c18_half_public_q0.py`): +3, -0 on the rows with a
  public leaf: CHIDIST df3 x=4, GAMMA.DIST a=1.5 x=2 (two rows).
- Still open: CHIDIST df3 at x = 3, 6, 8, 16, 32 and df5 at x = 6, 16, 32; GAMMA.DIST a=1.5 at x = 1.5, 3.
- Diagnostic only, not landable: T53 with C = 6c (the answer's mistaken constant) scores +9, -0 on the same 30 rows
  (`c15`, `c17`). It fixes every row where ours is high and worsens the y=8 rows where ours is low by 3 to 6 ulp. It
  has no natural origin (FLDPI and sqrt-of-double-pi give 6b or 6a, `c16`), so it is a fitted constant. It does show
  the remaining positive-d residual sits in the first term or the Q0 leaf.

**R5. Stored generic seed 6a at y = 0.5 (C.3/C.8).** dc-6332.7, not .3.
- Bank effect: +2 (GAMMA(-0.5), GAMMA(-1.5)), -1 (GAMMA(-2.5)).
- There is no computation behind it other than a special value. The common-seed graph is refuted by the three bank
  rows (claim 12). Do not land. Carry the three rows into B.4c.

**R6. C.3 nine-exception candidate, restated.** dc-6332.3, low priority.
- Computation: in `gamma_stirling`, pass the unstored R64(w/n) from `stirling_series` to the register EXP body
  (`exp_ext` exists in `x87_chains.rs`). Store nothing between them.
- Reach: up to 9 GAMMA rows in [12, 48). Risk: the 5,198 matched x >= 10 rows.
- The answer itself says it may explain none.

**R7. Inverse facts from the bank, for dc-6332.13 (no candidate to land).**
- Exponential forwards: Excel returns the correctly rounded true quantile. CHIINV df2: 10/10 (ours 3/10). GAMMA.INV
  a=1, b=1: 10/10 (ours 5/10).
- ERF and ERFC forwards: not correctly rounded. CHIINV df1: 2/10, off by -2..+5 ulp. GAMMA.INV a=0.5: 3/10
  (`c10`, `c11`).
- A special case `-2*LN(p)` (CHIINV df2) and `-b*log1p(-p)` (a=1) would gain about 12 rows. Treat that as a hypothesis
  B.9 can test, not a replay: an accurate solver predicts the same bits.

Priority by rows reachable: R3 (≤494, uncertain) > R2 (32 + GAMMALN rows, needs B.3) > R1 (+11 forward, measured
+18/-4 in R35) > R7 (≈12, deferred) > R6 (≤9) > R4 (+3) > R5 (net +1, refuted).
By expected value, R1 comes first: it is certain and identified.

---

## 5. Where the answer is wrong, overstated or in conflict

1. **The sqrt(pi) audit is wrong** (A.4 paragraph, C.1 rows 7-8, C.5 constant, reference R8 misapplied). See claim 13.
   The answer's A.4 instruction "do not unify them under RN(sqrt(pi))" rests on a constant pair that does not exist.
   It inherited the prompt's misprinted decimal (appendix I, `1.7724538509055159`, which parses to 6a).
2. **Hidden-half deduction overstated** (A.3, B.4, C.8). The two-row deduction is exact, but "exactly the supplied
   Excel value… conditional on the recurrence and stored seed" ignores GAMMA(-2.5), which the prompt lists as matched
   (appendix E). The bank refutes any common seed under the coded chain. That is R33's finding (61/288 groups) again,
   which the answer cites but does not apply here. Curiosity: 6a is exactly sqrt of double pi, so "Excel computes
   sqrt(pi) at run time for some negative halves" is another candidate. It fits -0.5 and -1.5 too, and fails -2.5
   (`c4_neg_half_alts.py`).
3. **C.3 x >= 10 conflates the two Stirling corrections** (claim 9).
4. **C.5 / A.4 assume Q0 is public ERFC** (claim 28). Our Q0 is `gratio(0.5,·)`'s a<1 continued fraction with NSWC
   `gam1`.
5. **B.6 / C.5 treat H53 against H64 as open**. R35 separated them (2519 against 2523 fresh) and archived the program.
   The answer says "if available it takes priority". It is available.
6. **D.8 step 1's local half is not new**: R33 ran the public-GAMMALN density injection (108/108). B.5's D(t) probe
   was also already used to pin s(a) (`discrete_dist.rs:58-63`).
7. **B.9c's third column is redundant** with the measured identity CHISQ.INV(p,2a) = GAMMA.INV(p,a,2) = st(2·unit)
   (prompt G.10, 300/300).
8. **A.3's "accurate Lanczos product" rank** is not contradicted by R45/R49. Those refuted Lanczos-style forms for
   GAMMALN's lower bands (12 cleared g=7 candidates; binade signatures for g = 5, 7, 607/128, 9), not for the GAMMA
   seed. A brief should say which function each refutation covers.
9. **C.2 "+0 at x=1" and "x-0.5 inside the register"** (claims 5, 7): harmless differences from the code; both
   described above.
10. **Overstated confidence, minor**: A.1 rates the half-integer and continued-fraction arms "variable" but C.5 calls
    T53/D53 "isolating" the first term. With the leaf wrong (item 4), the T53/D53 contrast mixes first-term and
    leaf effects.

Where the answer touches answer 10 (beta) through shared code:

- `bisect_inverse` (`special_math_common.rs:2511`) also serves BETA.INV (`beta_gamma_stats_family.rs:80`) and FINV/TINV
  (`chi_f_t_family.rs:206, 223, 300, 327`). Answer 9 C.7 and answer 10 C.6 both ask for Newton and Halley harnesses:
  build one shared harness (extend, never duplicate) with per-family forwards.
- Answer 10 C.4 proposes routing `bgrat`'s Q(b,z) and prefactor through "the established gamma upper-tail core"
  (`gratio`, `poisson_mass`). It calls that core "already successful". Answer 9 changes that core (R1, R3, R4). If
  both land, every gamma replay must also measure BETA.DIST/BETADIST/FDIST/F.DIST/TDIST, and the order matters. Answer
  10's "reuse exact existing Stirling constants" also conflicts with nothing in answer 9 (C.4 is diagnostic only).
- No contradiction found between the two answers on Loader pieces: both keep the existing table, the series and the
  running exponential.

---

## Top five actions by expected value

1. **dc-6332.3 (R1)**: re-apply the parked R35 integer polynomial, keeping `GRATIO_RTPI_FULL`, and measure the full
   bank including the inverses and the beta family. Proxy: +11 forward rows, no forward losses.
2. **dc-6332.7 (B.3, with R2 as its acceptance target)**: recover B3 so that x = 2.5 and 2.75 match Excel. That flips
   up to 32 GAMMA.DIST cumulative rows plus GAMMALN at 2.5, 2.75, 3.75 and 6.5. Then B.2 for B1/B2.
3. **dc-6332.3 (R3)**: the C.6 resident-prefactor replay on the 542 large-mean POISSON misses (≤494 reachable),
   reporting the sign test and the losses among the 273 matched rows.
4. **dc-6332.7 (B.1 then B.4 with W(-2.5), W(-3.5) added)**: the a<1 normaliser question (11 a=0.75 rows) and the
   below-one seed, now known to fail as a common seed on the bank.
5. **dc-6332.3 (R4)**: switch the half-integer Q0 to the public ERFC.PRECISE leaf (+3 bank rows by leaf proxy). Then
   B.7 in dc-6332.7 with C = 6b, not 6c.
