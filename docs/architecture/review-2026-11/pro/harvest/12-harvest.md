# Harvest of answer 12 (FORECAST.ETS reconstruction)

Reader: Opus, dc-6332.1, 2026-11-10. Sources: `pro/12-ets-answer.md`, `pro/12-ets-prompt.md` (appendices A to F),
`PRO9_PROGRAMME.md`, beads dc-6332.5, dc-6332.9, dc-6332.13, the R37 to R48 work reports in
`codex-runs/pro-9/history/` and `DnaCalc-Research/design/ets-search.txt` (81fae57, the R46 to R48 design note).

Pinned code: `crates/calc-func/src/catalogue/statistics/ets.rs` at 1ece3d0f. `git log 1ece3d0f..HEAD` on that path is
empty and the file is byte-identical at HEAD, so every line number below holds at both commits.

Checking method: no cargo, no Excel. I wrote an offline Python emulation of the pinned nonseasonal path (`regression_slope`,
`initial_slope`, `Model::run`, `finish` with exact RN53(RN64()) emulation of `excel_x87_*`, `statistics`, the search,
forecast and CONFINT). It reproduces E.6 (H74) exactly, the matched n=3 and n>=8 corpus rows I tried (H86, H22), and all
four T-probe hand calculations. Scripts and outputs are in `C:\Work\DnaCalc\codex-runs\pro-9\work\harvest-12\`:
`ets_sim.py` (emulator), `check_short.py`, `corpus_short.py` (replays the three ETS captures), `check_a3.py`,
`check_ties.py`, `check_det.py`, `h95_scan.py`, `h95_variants.py`, `predict.py`.

## Headline: the short-initialisation block does not need a new initialiser

The answer treats the 204 short-initialisation cells as blocked on an unidentified kernel (A3 "H/open", C1, C2, D7), and
says the p2,n4 routing "does not establish forty new matches" because those histories "enter the unresolved nonexact
short initializer". Replaying the corpus offline says otherwise. The arithmetic **already in ets.rs** (B.2's binary64
`initial_slope` and its seven-point extension, `ets.rs:328-338`) reproduces Excel on almost every refused short cell; it is
only the exactness guard `exact_short_history` (`ets.rs:365-378`, applied at `ets.rs:686-691`) that refuses them.

`corpus_short.py`, over the three recorded captures (every cell classified with the pinned routing, then computed with
the guard removed; CONFINT uses z = 1.9599639845400536, which the existing matched CONFINT rows support):

| Change | Cells moved from refused to calculated | Match Excel | Mismatch |
|---|---:|---:|---:|
| (a) drop the `exact_short_history` guard, keep the binary64 extension | 204 (FORECAST 66, CONFINT 66, STAT 72) | 199 | 5 |
| (a) + (b) explicit period 2 at n=4 goes nonseasonal (answer C1) | +40 (FORECAST 20, CONFINT 20) | +40 | 0 |
| (a) + (b) + (c) automatic seasonality at n<=4 goes nonseasonal (R47: automatic equals period 0 on 192 histories of length 2..4; SEASONALITY is 0 there) | +40 (FORECAST 20, CONFINT 20) | +40 | 0 |
| **Total** | **284** | **279** | **5** |

The classification count for (a) is exactly the census's 204, so the port of the routing agrees with appendix C. The five
mismatches are all H95 (n=7): STAT 4, 6, 7 (3, 2, 1 ulp), FORECAST (2 ulp), CONFINT (1 ulp). Alpha and beta match. The two
cycle-limit histories match exactly: {2;7;-1;4} gives FC 1.5926883521785709 and CI 6.140512107538674, and
{2;3.5;3.75;5.75} gives FC 6.678586669633923 and CI 0.7038800985387762, as in E.5(b), although their extensions are
non-dyadic (1.2, 6.2, 0.7999999999999998). Refused cells would fall from 952 to 668 (FORECAST 150, CONFINT 150,
SEASONALITY 368, STAT 0).

This agrees with earlier research. R47 (`ets-search.txt`) found the same fixed raw-moment, first-two-phase extension
"matches 960 reported parameters but fails 27/1,280 forecasts". At that time only proved regions landed. The pro-9 landing
rule (a clear net gain, losses listed, zero loss not required) admits it. FORECAST.ETS has no bank rows (work reports
38 to 40), so the corpus (`cargo xtask corpus`) is the measure. My numbers come from an emulator, not the Rust build;
the dc-6332.5 lane must reproduce them with `cargo xtask corpus` before landing.

## 1. Disposition of every section

| Section | Content | Disposition |
|---|---|---|
| A1 | Validation and preparation rules | No action: every M row matches `ets.rs:85-212, 775-849` (section 2). Its open items (calendar, fractional horizons, reversed-input pre-first target) stay with dc-j9k. |
| A2 | Routing, automatic detection family (spectrum plus correlation), H105/H127 exclusion | Detector: **dc-6332.13**. Routing at n<=4: **dc-6332.5**. Change (c) above follows from A2's own M line, but the answer does not spell it out. |
| A3 | Short initialisation, exact weight table, H90/H95/H100/H97/H87 slopes, the "final division cannot flip" argument | **dc-6332.5**: drop the guard (headline), and keep the weight table as a diagnostic in the harness. Every number checks (section 2). |
| A4 | Recurrence, validation window, whole-history SSE, separate application pass | No action: identical to `ets.rs:380-386, 441-461, 697-713`. |
| A5 | 25-point pool decomposition, bounds, search-shape ranking, 0.126/0.125 midpoint clue | **dc-6332.9** through B2's probes. No code change. |
| A6 | Nonseasonal reset, centering | No action: identical to `ets.rs:463-497`. Generalisation to period p: **dc-6332.13**. |
| A7 | Forecast, STAT 1-8, CONFINT, general AAA variance | Nonseasonal: no action (identical to `ets.rs:716-760, 500-528`). General seasonal variance: **dc-6332.9** (it is read through C3/D4), then **dc-6332.13**. |
| A8 | Seasonal initial states (B.5), noisy cases carry no gamma information at h=1 | **dc-6332.9** (B5, S0, C3). The claim about h=1 holds (V1 = 1). |
| B1 | Candidate-generation ranking; 812/1362 controls; decimal-load vs output-quantisation | 812/1362 replays with F1-F3 and scaled copies (36 calls, no predictions): **dc-6332.13**. Gain-load test by D3: **dc-6332.13**. |
| B2 T0 | Nonzero tie, every candidate SSE exactly 202 | **dc-6332.9**, ready (6 calls with the scaled copies). |
| B2 T1 (+2 neighbours) | Controlled 0.75/0.9 tie, (0.833333,0.001) membership test | **dc-6332.9**, ready (6 calls). |
| B2 T2 | Same tie made a SSE-accumulation precision test | **dc-6332.9**, ready (1-2 calls). |
| B2 tolerance controls | Scaled and amplitude-halved T1 variants | **dc-6332.13**: amplitudes not specified, predictions qualitative. |
| B3 D0 | Spectrum vs ranks (S and its cube, 32 points) | Bead dc-6332.13 lists D0-D2 as deferred. I recommend moving D0 and D1 (6 calls in all, discrete outputs) into **dc-6332.9**, as D7 item 3 already asks. |
| B3 D1 | 2p<n vs 2p<=n eligibility | As D0: **dc-6332.9** (4 calls). |
| B3 D2 | Reversal and timeline dependence | **dc-6332.13** (11 calls; only useful once a detector candidate exists). |
| B4 | Short initialisation ranking and experiments | **dc-6332.9**, re-scoped: 11 of the 12 histories are already explained by the existing arithmetic, so the informative targets are H95 and fresh short histories (section 3). |
| B5 | p=4 initializer probe (y=256j, +48 at j=8) | **dc-6332.9** (12 calls). Interpreting it needs the D2 inversion from **dc-6332.5**. |
| B6 S0 | Seasonal exact tie {0;8;0;8;0;8;0;8+d} | **dc-6332.9**, ready (6 calls). |
| B6 S1 | 16-history p2 survey | **dc-6332.13**: the answer gives no predictions, and the programme requires them frozen first. |
| B7 | Explicit cycle limits, P3/P4/P5 | **dc-6332.9**, ready (10 calls). |
| B8 | Seasonal statistic conventions; gamma convention through C1,C4,C5; seasonal reset through +-256j | Covariance test: inside C3 (**dc-6332.9**). Statistic conventions and seasonal reset: **dc-6332.13**. |
| C1 | Readiness table; p2,n4 routing; exact seasonal branch unchanged | Routing: **dc-6332.5** (with changes a and c). Its "zero additional matches" holds only while the guard stays (headline). |
| C2 | Short-history scalar-B0 identification procedure | **dc-6332.5** harness, aimed at the 5 H95 residuals and at R47's 27/1,280 fresh failures. R48 already ran its step 3 for 9 histories (no compatible B0 in 513 neighbours), and so did I for H95 (none in +-4,000 ulps; up to 6/7 outputs agree). Go to step 5 first. |
| C3 | 105-reading seasonal batch on the nine E.5(b) histories | **dc-6332.9**, ready. |
| C4 | Predictions for 16 histories | No action: all 16 are cells we already match (n=3 nonseasonal and exact p2 seasonal), so they test nothing. |
| C4 detector control | Exact-period finder as a negative control | No action: it is a refutation and it checks (section 2). |
| D1 | Confidence 1-2^-53 makes CONFINT(h=1) = sqrt(SSE/n) | **dc-6332.5** (harness reader) and **dc-6332.9** (controls, predictions in section 3). |
| D2 | State recovery from forecasts; inverse recurrence | **dc-6332.5** harness. |
| D3 | Gains from perturbation responses | **dc-6332.13**. |
| D4 | Covariance gains from CI increments | **dc-6332.9** (read inside C3), **dc-6332.5** (reader). |
| D5 | Winner-region witnesses (quadratic forms) | **dc-6332.13** (method; T1 is its one worked case). |
| D6 | Duplicate aggregation order, missing points, magnitude extremes | **dc-6332.13**. The duplicate probe is already answered (section 5). The 2^-600 underflow probe is cheap and could go with T2. |
| D7 | Order of work (about 130 formulas) | **dc-6332.9** for the order; item 2 re-scoped as in section 3. Its "first implementation target should be the short initializer" agrees with the headline. |

No new bead is needed. dc-6332.5's text should change: changes (a)+(b)+(c) are expected to add about 279 matches and 5
mismatches, not zero. Items for dc-6332.13: B1 controls, B2 tolerance controls, B6 S1, B8 statistics and reset, D2
reversal/timeline, D3, D5, D6.

## 2. Claims about our code and the prompt's data

| Claim (answer section) | Verdict | Evidence |
|---|---|---|
| #N/A for unequal lengths precedes option checks (A1) | Agrees | `ets.rs:775-777` before `ets.rs:801-810`; `prepare` repeats it at `ets.rs:92`. |
| Missing-point rule 10(N-D) > 3N, equality admitted (A1) | Agrees | `ets.rs:175`. |
| Interpolation yL + (yR-yL)(j/g) (A1) | Agrees | `ets.rs:196`. |
| Step = smallest gap; STAT 8 and confidence 0 after the validations (A1) | Agrees | `ets.rs:151-154, 844-849`; these come after the target and horizon checks at `ets.rs:817-843`. |
| 128 points, 32 fitted, period <= 8, 2^+-400 are our own guards, not Excel limits (A1) | Agrees | `ets.rs:172, 545-555, 670-682`. |
| Explicit period > floor(n/2) is nonseasonal; p2,n4 already nonseasonal only when affine (A2, C1) | Agrees | `ets.rs:863-865`: `short_affine` requires `exact_trend`. |
| Extension E_j = y_(j mod 2) + (j - j mod 2) r to seven points; level y0; seasons 0; gamma 2^-52 (A3) | Agrees | `ets.rs:328-338, 409-423`. |
| Search on the extended history, application on the real one; n=7 has no extension but the guard can refuse it (A3) | Agrees | `ets.rs:686-691, 712`; `exact_short_history` runs `exact_regression_slope` even at n=7 (`ets.rs:342-378`). |
| Weighted-slope table for n=3..7 (A3) | Agrees, all five rows | `check_a3.py`, exact rationals. |
| H90 181/112, H95 -11/7, H100 -117/448, H97 9/4 then 429/224, H87 -359/80 then -893/224 (A3) | Agrees | `check_a3.py`. |
| Odd denominator 7 keeps >= 1/14 ulp from a midpoint, so RN53 vs RN53(RN64) of the final division cannot differ (A3) | Agrees | For n=7, n*sxy - sx*sy = 7(sxy - 4sy), so the 196 denominator reduces to 28 = 4*7. The binary64 slope of H95 equals RN53(-11/7) (`h95_scan.py`). |
| Recurrence shape; level += (trend + alpha*e); no FMA (A4) | Agrees | `ets.rs:441-453`. |
| w(m) = m for m <= 10, min(10, floor(0.4m)) otherwise; no frozen window for m <= 10; SSE over all errors incl. e0 = 0 (A4) | Agrees | `ets.rs:380-386, 459-461, 697-698`. |
| Statistics from the application pass, saved before reset (A4, A6) | Agrees | `ets.rs:712-715`. |
| 25-point pool decomposition 7+6+5+7; B.2 holds 23 (A5) | Agrees | `ets.rs:283-307`; E.1. |
| (.002+.25)/2 = .126, (.001+.249)/2 = .125 (A5) | Agrees | Arithmetic. |
| Reset: windows n or ceil(n/2), divisor n or floor(n/2), stored-PC64 raw moments, threshold, level/trend replacement, beta .001, centering (A6) | Agrees | `ets.rs:463-497`. NaN from a negative variance makes the comparison false, so no reset, as the answer warns. |
| CONFINT uses the corrected beta, uncorrected alpha, full-history SSE/n (A6, A7) | Agrees | `ets.rs:711-715, 736-759`. |
| Forecast XADD(fl(L + fl(hB)), S_(n-1+h) mod 2) (A7) | Agrees | `ets.rs:721-727`. |
| STAT 4-7 definitions; MASE prefix u (A7) | Agrees, with one gap: the STAT 5 row omits that SMAPE also runs over j = start..n-1 only | `ets.rs:500-528` (`ets.rs:509`). |
| Nonseasonal V_h and its gamma term; q = (1+c)/2; z = 1 at q = 1; XMUL publication (A7) | Agrees | `ets.rs:737-759`. The impulse-response form in D4 sums to the same closed form; for p = 2 it gives `g*k*((2a+g)+2b(k+1))`. |
| General AAA variance with gamma*k*(2a + gamma + p*b(k+1)) (A7) | Agrees with Hyndman et al. 2008 class-1 ETS(A,A,A); unmeasured for Excel, as stated | Mathematics. |
| B.5 initial states and default triple (.751,.001,.001) (A8, C1) | Agrees | `ets.rs:568-629`. |
| E.5(b) has six p2 and three p4 noisy histories with n >= 8 (A8, C3) | Agrees | Prompt lines 1238-1246. |
| The exact-period finder predicts 2,2,4,8,2,0,2,2,4,0,2,4,0,8 for H102-H115 (C4) | Agrees | `check_det.py`. |
| "Both enter the unresolved nonexact short initializer" (C1) | **Disagrees in effect** | They enter `exact_short_history` (`ets.rs:687`) and are refused, but the unguarded code computes Excel's exact FC/CI for both (headline). |
| The p2,n4 routing alone adds zero matches (C1, principal finding 1) | Agrees for the pinned code | `ets.rs:687` refuses both histories (OLS -1/5 and 23/20 are non-dyadic, `check_a3.py`). |
| C1(c*) = sqrt(SSE/n) and equals STAT 7 for n <= 10 (D1) | Agrees | (1 + (1-2^-53))/2 rounds to 1.0 in binary64 (ties to even), so `ets.rs:748` gives z = 1, and V1 = 1. Both STAT 7 and SSE fold the same errors including e0 (`ets.rs:459-461, 507-508`). The emulator gives C1(c*) = STAT 7 for all 12 E.4 histories, and differs for long ones (H22: 0.7548066628870962 vs STAT 7 0.3977510795736776). |
| D2 inversion identities, gauge normalisation (D2) | Agrees | Algebra: L_j + S_new = y_j - (1-a-g)e_j follows from the update order at `ets.rs:443-452`. |

Constructed examples (section B), checked with exact arithmetic and with the emulated binary64 code:

| Example | Verdict | Detail |
|---|---|---|
| T0: OLS preserved (12*13 - 78*2 = 0); every candidate SSE exactly 202 | Agrees | Initial slope exactly 1000.0. All 30 E.1 pairs give 202.0, also at 2^-100 and 2^100 scale. Picks: B.2 strict-first .9/.001; B.2 last-retained .998/.899; 25-pool last-retained .998/.249; no reset. |
| T1: perturbations preserve OLS (3*2000 + 5*26170 - 7*19550 = 0); d = (13.085,-9.775,0,0); best real alpha .825 at b = .001 | Agrees | |
| T1: A and B validation errors; both scores exactly 1,060,323,240 | Agrees | Exact binary64 integers in the emulated recurrence. |
| T1: all other pool points score higher | Agrees for the 25-pool | The reset-only (0.833333,0.001) scores 1,060,234,351.02 and would win if evaluated, as the answer says. The next best are the reset-only (0.666667,.001) and (0.998,.001). |
| T1 +11/-9 favours B by 1200; -11/+9 favours A by 1200 | Agrees | Emulated SSEs 1,060,316,254 vs 1,060,317,454, and 1,060,329,430 vs 1,060,330,630. |
| T2 literal series; S = 1,060,323,240 + 130,540K + 6K^2; spacing 64; S mod 64 = 40 | Agrees | S = 432,380,606,852,317,096. |
| T2: binary64 products and forward sum give A = S-40, B = S+24; exact PC64 gives both S+24; binary64 products with an extended accumulator give A smaller | Agrees | Product rounding errors A (0,-4,-4,0), B (0,+28,-4,0) exactly as stated. The emulated code picks .75 and resets beta. |
| D0: S and T have zero sum and zero OLS slope; ranks identical; DFT amplitudes 4(A+2B), 4(2A-B), 4(A+8B), 4(8A-B) per cycle; T's power ratio about 3.09 | Agrees | Ratio 3.0924 (`check_det.py`). Amplitudes are per cycle; over four cycles they scale by 4. |
| D1 outputs under 2p<n, 2p<=n, three cycles | Agrees | |
| B5 predicted states: (0,256,0,0,0,0); (4,256,12,-4,-4,-4); (-17/4, 257.5, 57/4,-13/4,-19/4,-25/4) | Agrees | `check_det.py`, exact. |
| S0: B0 = 0, L0 = 4, S = (-4,4), only the last error is nonzero | Agrees | |
| B7 predicted gamma table C1/C2/C3 | Agrees with its stated rules | Wording: the patterns are not "exactly centered" (P3 mean 4, P4 10, P5 14). This is harmless: the first-cycle level absorbs the mean and the states sum to zero. |
| D6: 2^53 + 1 rounds to 2^53 in forward binary64 | Agrees | But R38 already measured Excel keeping the unit (section 5). |
| D6: 30% boundary pair (10 grid points; 7 vs 6 observed) | Agrees | `ets.rs:175`. Already measured (A1). |

## 3. The proposed Excel experiments

Service limits (`tools/excel-service/README.md`): FORECAST.ETS arrays take at most 128 cells and at most 32 rows or columns.
**No experiment exceeds the limit**. D0 sits exactly at 32 values. Numbers travel as Value2 cell bits and request formula
text is refused, so constants needing 16-17 digits (1-2^-53, 2^53, 2^-600 scalings, the E.2 clouds) pass exactly as bits.
D1's advice to "use the Excel expression (1-2^-53)" can neither be followed nor is it needed: pass bits 3fefffffffffffff.

| ID | Inputs | Calls | Predictions per hypothesis | Ready? |
|---|---|---:|---|---|
| T0 | y = 10000..19000 step 1000, then 20011, 20991; t = 1..12; seasonality 0; STAT 1, 2; repeat x2^-100, x2^100 | 6 | First retained: .9/.001. Last retained, B.2 order: .998/.899. Last retained, 25-pool: .998/.249. The scaled copies give the same pair (verified). Cannot separate the B.2 order from R47's stable-first order: both start with (.9,.001). | Ready |
| T1, T1+, T1- | T1 as printed; index 10 +11 / index 11 -9; and the opposite; STAT 1, 2 | 6 | First retention: .9, .9, .75. Opposite precedence: .75, .9, .75. **(.833333,.001) with no reset means it is a search point.** No reset in any pick (verified). | Ready |
| T2 | Literal series in the answer; STAT 1 | 1 | Binary64 SSE: .75. Exact PC64 accumulator: tie, so .9 under first retention. Binary64 products with an extended accumulator: .75. Beta resets, so read alpha only. Interpret only after T1 settles precedence. | Ready |
| B2 tolerance | T1 balanced variants scaled 2^k, k in {-100,0,100}, amplitudes halved | open | Qualitative only | Needs filling (amplitudes, predictions) |
| D6 magnitude | T1- scaled 2^-600; STAT 1 | 1-2 | Binary64 squares underflow, all scores 0, default .9; an extended or normalised score keeps .75 | Ready; cheap, worth adding to T2's batch |
| Short F2/F3 (D7 item 2, B4 rank 1) | H87, H90, H95, H97, H100 (answer); I recommend all 12 E.4 histories; seasonality 0; FORECAST at n+2, n+3 | 10 (24 recommended) | The unguarded current program predicts, e.g., H90 F2 = 12.750007754513241 (402980010432c2f4), F3 = 14.365763912820523 (402cbb4568573e06); H95 F2 = -3.682483863804544, F3 = -5.275717317916456. All 24 are in `predict.py` output. Rival initialisers have no numeric predictions in the answer. | Ready with our predictions frozen; this becomes the fresh check of change (a). The SLOPE/LINEST comparison in B4 rank 1 needs filling (which outputs; extended values for n<7). |
| B4 rank 2 | H97 with offsets 0, 2^20, 2^40 and power-of-two scalings; STAT 1,2,4-7, F1-F3 | about 27 per scale set | Translation-invariant slope vs centring arithmetic; our program supplies exact predictions | Needs filling (scales); predictions can be frozen with `ets_sim.py` |
| D1 endpoint | Confidence = bits 3fefffffffffffff, h = 1; one short nonseasonal (e.g. H90), one long (e.g. H22), then one noisy seasonal (E.5(b) p2 n=12) | 3 | H90: 0.4073914833743825 (= STAT 7). H22: 0.7548066628870962 (sqrt(SSE/n), not STAT 7's 0.3977510795736776). Seasonal: no prediction. | Ready (histories filled in here) |
| D0 | S and T (each 8-cycle repeated 4 times, 32 values); SEASONALITY | 2 | Largest peak, rounded: (8,3). Smallest perfect Pearson or Spearman lag: (8,8). Rank-only: equal outputs. | Ready |
| D1 | {1;-2;1;1;-2;1}, the same plus 1, {1;-1;-1;1;1;-1;-1;1}, the same plus 1; SEASONALITY | 4 | 2p<n: 0,3,0,4. 2p<=n: 3,3,4,4. Three cycles: 0,0,0,0. Other gates can also give 0. | Ready |
| D2 | H105, H111, H139 reversed; D0 values on four timelines (i, 7i, 45000+i, 45000+i/4); SEASONALITY and STAT 8 | 3 + 8 | Reversal-invariant: 0,4,3. Timeline-blind: the same period; STAT 8 = 1,7,1,0.25 | Ready, deferred (.13) |
| B5 | p=4, y = 256j (j=0..11) with +48 at j=8; STAT 1-7, F1-F5 | 12 | Initial-state table (checked). Comparing needs D2 inversion with the read parameters, then forward replay. | Ready to ask; needs the dc-6332.5 inversion to read |
| S0 | {0;8;0;8;0;8;0;8+d}, d in {1/16, 1, 16} (d=1 is the base series); p=2; STAT 1, 3 (add 6, 7) | 6 (12) | Prefix initialiser plus first retention: .751 / .001; MAE d/8; RMSE d/sqrt(8). Any departure means later data reach the initialiser or the score differs. Reset fires (the last trend is beta*d), so beta reads .001 either way. | Ready |
| S1 | 16 p2 n=10 histories (u,v in {0,1,4,16}); STAT 1-3 | 48 | None given | Needs predictions frozen first (.13) |
| B7 | P3 at n=6,7,9; P4 at n=9; P5 at n=10 (repeated/truncated); explicit period = p; STAT 3, STAT 1 | 10 | Gamma: C1 .001 x5; C2 .001, eps, .001, eps, .001; C3 eps, eps, .001, .001, eps (eps = 2^-52 means a nonseasonal fallback) | Ready |
| C3 | The nine E.5(b) noisy histories; STAT 1-7; F2..F(p+1); C_p, C_(p+1) at 95% | 105 | No numeric predictions; it fills in missing observables. The covariance check (B8/D4): (C_(p+1)^2 - C_p^2)/C_1^2 = (a+pb+g)^2 for an innovation gain, (a+pb+(1-a)g)^2 for the traditional form | Ready |
| B8 reset | Noisy y and y +- 256j, explicit period | about 6-12 | Qualitative | Needs filling (which y) |
| B1 controls | E.2 cases 812 and 1362, as printed and scaled; STAT 1-3, F1-F3 | about 36 | None (validation set for any future generator) | Ready, low value (.13) |
| D6 duplicates | {2^53; 1; -2^53} at t=1, then 1,0,1,0,1,0,1 at t=2..8; SUM; permutations | about 9 | Forward binary64 0; wide 1 | Already answered for SUM/AVERAGE (section 5); only the permutation part is new |
| D6 missing | {0;2;..;7} on {1;3;..;8} vs 0..7; 30% pair | about 8 | Equal outputs; admitted vs #NUM! | Ready, but re-measures R38 |

D7's "roughly 130" is right (9 + 10 + 6 + 105) before the scaled copies and endpoint controls.

## 4. Directly implementable changes (no new Excel answers)

1. **Drop the short-history exactness guard** (`ets.rs:686-691`): build the search history with the same binary64
   extension `initial_slope` already uses (`ets.rs:332-337`) instead of `exact_short_history`. Cells currently passing the
   guard are unchanged: when everything is exact the binary64 extension equals the exact one. Effect: 204 short cells
   calculated, 199 match, 5 mismatch (H95). Owner: dc-6332.5.
2. **Route n <= 4 to the nonseasonal fit for FORECAST/CONFINT/STAT** (`ets.rs:863-865`): replace `short_affine` with
   `series.values.len() <= 4`. Affine histories still take `exact_trend` first (`ets.rs:884`). This covers the answer's
   p2,n4 rule and the automatic n<=4 case, which R47 measured. On its own: zero corpus change. With change 1: +80 cells,
   all matching. Owner: dc-6332.5.
3. Tests that assert the old refusals and must change with 1-2: `crates/calc-func/tests/f1_stats.rs:254-260`
   ({2;3;5;7}, seasonality 0, FORECAST at 5; no recorded Excel value, one Excel call needed) and the "Routing does not
   supply the still unidentified rounded initializer" assertion near `f1_stats.rs:3032-3042` ({2;7;-1;4}, p2; Excel's
   value 1.5926883521785709 is in E.5(b)). Replacing a refusal assertion with a recorded Excel value strengthens the test.
4. Harness pieces (no corpus change; they read future experiments): the D2 forecast-to-state recovery and inverse
   recurrence, the D1 endpoint reader, the C2 joint-outputs B0 scan (step 5 first: recurrence and STAT accumulation
   boundaries, since B0 alone cannot fix H95). Owner: dc-6332.5.

Combined effect of 1+2 on the 952 refused cells: 668 remain (automatic detection 488 = 528 - 40; nonzero seasonal
innovations 180; short and cycle blocks 0). Corpus matches +279, mismatches +5. Nothing in the answer offers a change for
the detector or the seasonal block without new Excel answers, and the answer says so correctly.

## 5. Where the answer is wrong, overstated, inconsistent, or in conflict with earlier rounds

1. **Short initialisation overstated as unidentified.** A3 marks the B.2 expression "H/open", C1 and principal finding 1
   say the cycle-limit cells "enter the unresolved nonexact short initializer", and C4 refuses to predict H87-H100. The
   existing arithmetic predicts 11 of the 12 E.4 histories bit for bit and both cycle-limit histories (headline). The
   answer could not run anything. The prompt's framing (refused "unless the OLS slope and the seven-point extension are
   exact") invited the reading. R47's own finding (960/960 parameters, 1,253/1,280 forecasts) already showed the shape was
   mostly right.
2. **C4 adds no information.** Its 16 "predictions" are cells we already match (n=3, exact p2 seasonal). The prompt
   asked for predictions on refused histories.
3. **B4 targets are mostly spent.** H90, H100, H97 and H87 already reproduce with the existing slope. Of A3's "isolating"
   histories, only H95 is informative. The answer's own A3 argument (the final division cannot flip) holds there; my
   +-4,000-ulp B0 scan on H95 and R48's 513-neighbour scans on nine histories both leave no compatible B0. So C2 should
   start at its step 5, not step 3.
4. **B5 ranks first a family that R48 refuted conditionally.** R48 tested 12 seasonal initialisers on 96 fresh
   histories, including "first-season states/first-two-cycle trend" and raw/centered regressions, with reported parameters
   as inputs; all have forecast counterexamples (forward/raw 57/384; R47 forward 2p 44/512). The answer had only the
   summary line in appendix D. The B5 probe is still worth running because its predicted states differ grossly. Do not
   expect the rank-1 family to survive C3's "reproduce all nine F1/C1" test.
5. **"Nonzero exact tie" is not new in kind.** R47 already found 147 exact-score ties (34 precedence constraints). T0 and
   T1 still add clean, designed, nonzero ties and the (0.833333,0.001) membership test. T2 (accumulation precision) is
   the genuinely new probe.
6. **D6 duplicate-order probe is already answered.** R38 measured that Excel keeps the unit for {2^53, 1, -2^53}:
   mean 1/3, sum 1 (`f1_stats.rs:261`, comment "Excel keeps the unit in this duplicate reduction"). Forward binary64 is
   refuted for SUM and AVERAGE already. Only the permutation variant is new. The 30% missing-point pair re-measures a rule
   the answer itself marks M in A1.
7. **D1 delivery detail.** "Use the Excel expression (1-2^-53)" does not fit the service: formula text is refused and
   numbers pass as exact bits.
8. **Minor wording.** B7 calls P3/P4/P5 "exactly centered" (they are not zero-mean; harmless). A7's STAT 5 row omits
   the window (`ets.rs:509`).
9. **Routing provenance.** The p2,n4 rule is R47's (`ets-search.txt`: "switch to the nonseasonal fit before five
   prepared observations, including explicit period 2 at length 4"), confirmed by R48's 3,840 readings. R47 also says
   "broader short routing was tried but exposed 68 new confidence and one RMSE numeric discrepancies" on research
   questions. The dc-6332.5 lane should read `DnaCalc-Research/evidence/round47` for what those 68 were before landing
   change 2. The corpus shows no loss from it, and the landing rule accepts listed losses.
10. **Internal consistency.** I found no contradictions between sections. The answer keeps detection eligibility (D1's
    2p<n) separate from explicit admission (B7's n>=2p), as its own A2 rule asks. Its D7 conclusion (short initialiser
    first) agrees with the headline.

## Top five actions by expected value

1. **dc-6332.5**: changes 1+2 of section 4 (drop the `exact_short_history` guard; n<=4 goes nonseasonal); confirm with
   `cargo xtask corpus` (expected 952 to 668 refused, +279 matches, +5 H95 mismatches); update the two `f1_stats.rs`
   refusal assertions; list the H95 losses in the commit and close note.
2. **dc-6332.9**: fresh check of change 1 before or with landing: F2/F3 and the D1 endpoint C1(c*) on all 12 E.4
   histories plus about 20 fresh random n=4..7 histories, predictions frozen from `ets_sim.py` (about 60 calls).
3. **dc-6332.9**: the T0 / T1 / T1+- / T2 / 2^-600 batch (about 15 calls), predictions as in section 3. It settles tie
   precedence, (0.833333,0.001) membership and SSE accumulation precision, the leading suspects for the E.2 tie-cloud
   failures.
4. **dc-6332.9**: the C3 seasonal batch (105 calls) with B7 (10), S0 (6) and B5 (12), the first observations of the
   180-cell seasonal block.
5. **dc-6332.5**: the D2 inversion and D1 reader in the harness, so that C3 and B5 can be read as initial-state and
   gain constraints; then C2 step 5 on H95.
