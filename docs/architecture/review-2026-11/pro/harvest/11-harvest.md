# Harvest of answer 11: solvers (YIELD, IRR, BINOM.INV/CRITBINOM, CUMIPMT/CUMPRINC, DURATION/MDURATION/PRICE)

Reader: Opus, 2026-11-10, for dc-6332.1. Answer: `pro/11-solvers-answer.md`; prompt: `pro/11-solvers-prompt.md`,
pinned to b2a80850 (master differs from it only in docs; every `file:line` below holds for both). Paths are under
`crates/calc-func/src/ported/` unless stated.

How the checks were done: reading the code; exact-rational Python for every bit claim (x87 `RN64` then `RN53`
emulated with fractions, `x87.py`); bank rows from `codex-runs/pro-9/v2/trace-<FUNCTION>`; history reports
`codex-runs/pro-9/history/work-report-<n>.md`; research evidence in `C:\Work\DnaCalc\DnaCalc-Research` (design notes,
`evidence/ld1/pvscan-big1.txt`, `evidence/ld1/binom-inv.patch`, `evidence/fable-52/i7_irr_two_5217*.json`,
`i1_irr_5211*.json`). Scratch scripts: `C:\Work\DnaCalc\codex-runs\pro-9\work\harvest-11\*.py`. Two simulators were
validated before use: the YIELD seed in plain binary64 equals our current output (`actual`) on all 68 near-seed rows
inside the prompt's seed thresholds (|price - 100| < 3e-5 short, < 7e-6 long, price != 100); the IRR secant simulator (x87 ops, `binexp_positive`, `power_kernel` rules) equals `actual`
on all 734 IRR rows. Where Excel's private power was needed (DURATION n = 1) a correctly rounded `pow` was used as a
proxy, labelled "(proxy)". No cargo, no Excel, no tracker writes.

## Findings that change how the answer should be used

1. **YIELD Y2 is a clean replay: +31 rows, no loss, computed exactly.** The answer's split denominator plus factored
   total coupon (B1 Y2 / C1 profile Y2) reproduces Excel on all 80 bank rows that take the seed path at price != 100
   (38 short, 30 long, 12 r18), against 50 today, and on 15 of the 17 price-100 rows of these designs (15 r18, one
   short, one long) when the par shortcut is bypassed (+r18 row 020, no loss; 024 and 026 stay basis-dependent). YIELD 66 -> 97 of 533. Y1 alone gives +30 (misses long-005).
   The x87-store variant of Y2 gives identical bits on all 97 rows.
2. **IRR: land the converted zero exit alone (+16, no loss); do not land the raw-guess residual.** Replacing
   `return Some(guess)` (`cashflow_rate_family.rs:494`) by `sub(div(1, before), 1)` moves IRR 533 -> 549 with zero
   losses. The answer's raw-guess initial residual (C2) has the analytic F0 values it claims (2^-43 and 0), but the
   full candidate returns `...ae0`, not Excel's `...ae8`, on the very row it was built for (irrB-lad-m2), breaks p4,
   and scores 530 of 734 (+30/-33). On Fable 5.2's 200 fresh two-flow problems it scores 139 against today's 149.
   A literal 20-update cap is refuted under our trajectory: three matched rows need 23, 43 and 52 updates.
3. **DURATION/MDURATION: the answer's binade hypothesis is strongly supported by the bank, and it is replayable now.**
   Every one of the 169 DURATION misses with n > 1 off-coupon has `S(off+(n-1)) != S(S(off+n)-1)`; none of the 7194
   rows where the two agree misses (59 flagged rows match). MDURATION: all 140 n > 1 misses flagged, 0 of 714 unflagged
   miss. PRICE does not follow (38 of its 50 misses are unflagged), as the answer warned. (A and E here come from
   the prompt-assembly schedule helper `work/solvers/yieldsim.py`, with the code's effective E per basis; month-end
   30/360 edge cases may differ from `days_between_num`, so a few rows could move between cells; the Rust replay
   decides.)
4. **DURATION n = 1 (8 misses) is a different defect the answer did not see.** Our n = 1 path returns
   `(off/f).max(0)` (`bond_core_family.rs:741-743`), independent of yield; Excel's n = 1 answers on one schedule take
   three different values across yields (46356/46446, basis 1: `...591b/591c/591d`). Excel runs the general weighted
   ratio. With a proxy power the general loop matches 25 of 30 n = 1 rows (today 22).
5. **CUMPRINC L1 is refuted by existing data; do not spend the Excel calls.** On the 2,000-pv scan of the same
   saturated singleton (r = 0.75, nper = 1000, period 1000; `pvscan-big1.txt`) the answer's leading R1 graph matches
   0 of 2000, the payment-first control 20, the rounded-reciprocal R2 graph 163, and no constant within +-40 ulp in
   any of the five two-rounding shapes covers more than 326 of 400. The answer itself asked for this check first.
6. **The one-period CUMIPMT puzzle is already decided by the bank.** Excel's CUMPRINC(8.05e-9, 1, pv, 1, 1, 0)
   (r15-cumprinc-036) is `0xbfaa950ab2586dfb`, two ulps from -pv (`...df9`, ours). Excel's CUMIPMT + CUMPRINC
   gives a payment bit-identical to ours (`0xbfaa950ab5ef7d10`). So the principal is not -pv and the payment is ours;
   the CUMIPMT miss is the CUMPRINC miss mirrored (+2^-56 / -2^-56). L2 shrinks to a 2-call confirmation.
7. **BINOM.INV/CRITBINOM: the retained patch is ready to land as a net gain** (2290 -> 2302, 1402 -> 1414, one loss
   each, measured in research). It no longer applies textually (`git apply --check` fails at
   `discrete_dist.rs:261` after the dc-ysl.10 reformat); port it by hand.

## 1. Section dispositions

Beads: **.2** = dc-6332.2 (local replay), **.6** = dc-6332.6 (Excel experiments), **.13** = dc-6332.13 (deferred).

| Section | Content | Disposition |
|---|---|---|
| Main conclusions 1 | YIELD seed associations | .2 (Y2 computed: +31) |
| Main conclusions 2 | IRR collapsed initial state | .2 (converted zero exit +16); raw-guess residual refuted (section 5) |
| Main conclusions 3 | BINOM.INV public-CDF rule | .2 (land research patch); CDF bits belong to the beta family (dc-6332.4/.8) |
| Main conclusions 4 | CUM* cancellation, second rounding | .2 for R0; L1 graphs refuted by existing data |
| Main conclusions 5 | Bond exponent construction | .2 (binade replay first), .6 (F1-F4) |
| S1-S11 statements | MS documentation priors | no action: priors only, already stated in the prompt |
| Arithmetic notation | S, E, X definitions | no action: matches our `excel_x87_*` (`excel_numeric/mod.rs:500-587`) |
| A1 skeleton | pre-update return, tau ~ 1e-10 | no action: already landed (`bond_core_family.rs:577-607`, 678-688) |
| A1 new evaluation order | `n*rate` total, split denominator | .2 (top YIELD replay) |
| A1 limit (024/026) | basis-blind seed | .6 (needs a basis-sensitive probe; no replay candidate) |
| A1 general seeds | finite-difference sign analysis, ranking | .2 profile replay on on-coupon b4-f1 (A = 0); off-coupon to .13 |
| A1 cash scaling | fixed yield-step FD is scale-free; negative root not reachable by residual stops from 2/17 | no action: correct deduction, informs Y6 (.6) |
| A1 objective/cap/errors | PRICE objective as candidate; CAP=100 hypothesis | .2 (objective profile), cap to .13 |
| A2 family | secant in v, conversion | no action: already the code (`cashflow_rate_family.rs:475-513`) |
| A2 initial state insufficient | collision pair | .2 done here (two collision groups confirm); .6 I1 |
| A2 raw-guess residual | `b_ext = E(1+g)` candidate | no action: refuted on bank and on Fable I7 (section 5) |
| A2 two deductions | zero exit, rate round trip exact on [0.5, 2] | .2 (zero exit); round-trip note: no action, correct |
| A2 stop and failure | dual stop, caps | .2 keep cap 200 (20 refuted conditionally); .6 I3/I4 |
| A3 | strict-zero/inclusive-interior predicate | .2 (land patch) |
| A4 | backward recurrence, R1/R2, type 1, one-period bound | .2 for R0 and type 1 finishes; R1/R2 no action (refuted); one-period bound moot (finding 6) |
| A5 | keep identified arithmetic; binade association; metadata catch | .2 (binade replay), .6 (F1) |
| B1 Y1 | association witnesses, cohort, raw-rate collision | .6 (witnesses and collision ready; cohort needs dates) |
| B1 Y2 | step-threshold brackets | .6 |
| B1 Y3 | annual vs periodic tolerance | .6 |
| B1 Y4 | redemption manifold | .6 |
| B1 Y5 | off-coupon generalisations | .13 (bead already lists it); local candidate tables when picked up |
| B1 Y6 | derivative, scale, domain, cap | .2 (derivative profiles, R44/R46 local replay), .6 (14 scale calls), cap to .13 |
| B2 I1 | guess ladder x 3 scales | .6 |
| B2 I2 | P vs vP | .6 |
| B2 I3 | residual-only / OR / AND | .6 |
| B2 I4 | double root, cap | .6 (lower priority: bank already bounds the cap below by 52 under our trajectory) |
| B2 I5 | sign, domain, d17/d19 frontier | .6 |
| B3 Q1 | small separators, CDF leaves | .6 (leaf part shared with the beta family) |
| B3 Q2 | large-n | .6 |
| B4 L1 | three-point money-rounding signature | no action: all five graphs refuted by `pvscan-big1.txt` |
| B4 L1 follow-ups | ranges, unsaturated loan, rates 1 and 3, type 1 | .13 until a graph survives |
| B4 L2 | one-period operands | .6, reduced to PMT and PPMT (finding 6) |
| B4 L3 | type 1 finishes, fractional bounds | .6 |
| B5 F1 | two/three-call zero-yield signature | .6 (fix the 15-digit hex first) |
| B5 F2 | private discount by several numerators | .6 |
| B5 F3 | stored-base collision | .6 |
| B5 F4 | fractional reuse vs full exponents | .6 |
| C1 seed association | Y0/Y1/Y2 code | .2 |
| C1 general-seed candidates | clean fractional annuity, quadratic first period | .13 (Y5) |
| C1 stopped iteration | CAP loop, providers, domain profiles | .2 for on-coupon non-par designs; full off-coupon to .13 |
| C1 analytic slope (X ops) | slope graph | .2 as a profile |
| C2 | IRR minimal candidate | .2: take only the converted zero exit |
| C3 | inverse binomial lower-bound search | .2 (equivalent to the research patch) |
| C4 wrapper | fractional bounds | no action: already the code |
| C4 R0 | backward recurrence (E.3) | .2 |
| C4 R1/R2 (R1a/R1b) | normalized quotient, money-first | no action: refuted (finding 5) |
| C4 type 1 finishes | COEFFICIENT_FIRST / PAYMENT_AFTER | .2 (both profiles; the 28-loss due loop is the warning) |
| C4 interest finish | remove period 1 before both operands | no action: already the code (`cumulative_finance_family.rs:119-123`) |
| C5 | time constructions T0/T1/T2, discount FULL/SPLIT/RECURRENT | .2 for T2 (binade); power profiles after F1-F3 (.6) |
| D1 stop frontier, seed regime | Y2-Y4 | .6 |
| D1 negative-root survey | medium | .6 |
| D1 derivative inference | medium | .6 after Y2/Y4 |
| D1 cap tomography | expensive | .13 |
| D1 reverse iteration | expensive | .13 |
| D2 state partition | cheap | .2 done here; .6 via I1 |
| D2 multiple roots | cheap | .6 (I4) |
| D2 sign reversal | cheap | .6 (I5) |
| D2 P vs vP | cheap | .6 (I2) |
| D3 CDF bits through the inverse | cheap/medium | .6, coordinated with dc-6332.8 (beta family Excel) |
| D3 disagreement certificates | medium | .6 |
| D3 identifiability limit | none | no action: a statement of limits |
| D4 money collisions | cheap | .2 done here (L1 refuted) |
| D4 cancellation amplifier | cheap | .6 (L2, reduced) |
| D4 fractional nper | medium | .13 (needs a surviving money graph) |
| D4 range partitions | medium | no action: nonadditivity already measured (R36: 10/25, 11/25) |
| D5 binade crossings | cheap | .2 then .6 (n = 3,4,7,8,15,16) |
| D5 rounded quotients | medium | .6 (F2) |
| D5 base/exponent collisions | medium | .6 (F3) |
| D5 zero-yield controls | cheap | .6 (F1 + the 16 schedules) |
| Proposed next round table | call budget by function | .6 (reorder per section 3 below) |
| Questions-answered table | summary | no action |

No new bead is needed: every item fits .2, .6 or .13. The n = 1 DURATION replay (finding 4) belongs in .2.

## 2. Claims about our code and the prompt's data

| # | Claim (answer section) | Verdict | Evidence |
|---|---|---|---|
| 1 | Y0 = present association `n*S(S(100*rate)/f)`, den `S(S(d*S(n+1))/2)+S(n*R)`, `f*(num/den)` (B1, C1) | agrees | `bond_core_family.rs:590-592`; the Python Y0 equals `actual` on all 68 in-threshold near-seed rows |
| 2 | Seed returned before update when step < 1e-10; analytic slope from `crt_pow` | agrees | `:596-607` |
| 3 | Unconditional par shortcut returns `rate` | agrees | `:640-647` (n > 1, coupon date, price = red = 100) |
| 4 | Internal guard rejects `y <= -f`; bracket low end `-f+1e-10` | agrees | `:352-354`, `:394` |
| 5 | YIELD inverts `pcomp(..., binexp=false)`, PRICE uses `excel_bond_pow` | agrees | `:330`, `:575` |
| 6 | n=3: Y1 total coupon `7.5 + 2^-50`, Y0 exactly 7.5; n=15 both 37.5 | agrees | computed |
| 7 | Long design: present den adds 1.5 ulps of 1500 and rounds to 2; Y2 adds 1.40625 (to 1) then 0.09375 (stays 1) | agrees | computed: 3/2, 45/32, 3/32 units of 2^-42 |
| 8 | Y1 table bits (four short prices, Y0 vs Y1/Y2) and long-005 bits | agrees, all 10 values | computed; the two "supplied" rows match bank rows r18-011 and near-seed short |
| 9 | r18 020 and 021 predictions `0x3fad70a3d70a3d72`, `0x3fad70a3d70a3d60` under the factored total coupon | agrees | computed (n = 5, f = 4): Y1 and Y2 both give them; Y0 gives `...71`, `...5f` |
| 10 | Combined candidate reproduces r18 003, 011, 020, 021 | agrees | computed |
| 11 | Y1 and Y2 cannot reproduce 024/026 | agrees | both give `...71` at bases 2 and 3 |
| 12 | Raw-rate pair `0x3fa9999999999999`/`...9a` both store coupon 2.5; Y0 identical, raw-rate seed differs | agrees | Y0 `0x3fa9999926e978ed` for both; Y2 `...ed` vs `...ee` at 100+2^-19 (neighbours `...fb/...fc`, `...e0/...e1`) |
| 13 | Y2 true slope `-68921/9842000`; boundary `3.645185e-5`; inner pair steps 9.99949e-11, 1.000224e-10 | agrees within 1e-4 relative | mpmath: slope -0.0070027433448; steps 9.99939e-11 and 1.00021e-10 |
| 14 | Y3 annual pattern seed/seed/iterate at 100.00003, seed/iterate/iterate at 100.00006 | agrees | steps f=1,2,4: 4.1e-11, 8.2e-11, 1.65e-10 and 8.2e-11, 1.65e-10, 3.3e-10 |
| 15 | Y4 seed `S(1/1535)` / `S(-1/1537)` for every R; error ~4.6e-11; at 2^-9 ~3.7e-10 | agrees | computed: `0x3f4558e426065664`, `0xbf4551c7b40ca88f` for R = 97, 100, 128; errors 4.6e-11, 3.67e-10 |
| 16 | Cash scaling bits `0x20f9...`, `0x2119...`, `0x5f79...`, `0x5f99...` | agrees | exponent arithmetic |
| 17 | n=2, rho=0.8 roots 0.118 and -2.118; annuity start 2/17 below the positive root, monotone | agrees | z0 = 19/17 = 1.1176 < sqrt(1.25) = 1.1180; Newton on a convex decreasing function stays left |
| 18 | Fixed annual-yield FD steps are bit-homogeneous under 2^k scaling | agrees | power-of-two scaling commutes with rounding while normal; corrects the prompt's B.7 suggestion |
| 19 | IRR guesses `...ade`/`...adf` both store `1+g = 0x3ff35c28f5c28f5c`, Excel `...ae8`/`...ae0` | agrees | bank irrB-lad-m2/m1 (and oc53/oc51); v0 `0x3fea723f789854a1` for both |
| 20 | Raw residual F0 = 2^-43 and 0; quotient errors 7.52e-14 and 5.23e-14 around 2^-44 | agrees | computed exactly |
| 21 | IRR code: secant in v, x87 ops, `v1 = v0 +- 0.001`, dual stop, cap 200, zero exit returns guess | agrees | `cashflow_rate_family.rs:475-513`, default guess 0.1 at `:517` |
| 22 | `S(1+S(S(1/v)-1)) == S(1/v)` for `S(1/v)` in [0.5, 2] | agrees | Sterbenz; makes the prompt's H2 a no-op for modest rates |
| 23 | d17 error 6e-13 in rate ~ residual 3e-9 | agrees | dP/dr ~ -4800 |
| 24 | BINOM.INV code: forward mass sum, strict `sum > alpha` | agrees | `discrete_dist.rs:290-318` |
| 25 | `S(1 - 0.50000000000000011) = F(2;5,.5) = 0x3fdffffffffffffe` | agrees | computed |
| 26 | CUMIPMT finish `x87_sub(x87_mul(count, payment), principal)` | agrees | `cumulative_finance_family.rs:119-123` |
| 27 | CUMPRINC type 0 is a plain sum of public PPMT; recurrence of E.3 not landed | agrees | `cumulative_finance_family.rs:72-117` |
| 28 | L1 signatures `b+2,b+2,b+3` / `b+1,b+2,b+2` / `b,b+1,b+2` / `b+1,b+2,b+3` / `b,b+2,b+2` | agrees (all five) | computed; but every graph is refuted on existing data (section 5) |
| 29 | One-period bound `pv*2^-53 + 2^-58 = 9.23e-18` < observed 1.566e-17 | agrees | computed; premise "principal = -pv" is false in the bank (finding 6) |
| 30 | DURATION: coupon PV x87 div, weight `diff*coupon_pv` plain, redemption `x87_div(diff*100, disc)`, final `x87_div(num,den)/f` | agrees | `bond_core_family.rs:747-758`; C5's `Xdiv(...,frequency)` equals the plain `/f` for f in {1,2,4} |
| 31 | MDURATION finish | agrees with a nuance | `:770` uses plain `1.0 + yld/f`; C5 writes `Xadd(1, Xdiv(y,f))`. Keep the plain sum (they differ only on double-rounding cases) |
| 32 | F.2 row (46845,47168,0,0.0839,4,3): tabular E=90 cannot be the effective E; 365/4 = 91.25 gives 0.88219 | agrees, and broader | `dc()` returns `365/f` for basis 3 and `360/f` for basis 2 (`bond_core_family.rs:229-237`); the prompt's F.2 A and E columns are calendar spans for every row (`work/solvers/gen_bond_tables.py` `sched()`), e.g. the PRICE basis-2 rows list E = 184 where the code uses 180 |
| 33 | F1: `tA = 0x3ffaaaaaaaaaaaa`, stored-weighting result `0x3feaaaaaaaaaaaa` | typo | 15 hex digits; correct values `0x3ffaaaaaaaaaaaaa` and `0x3feaaaaaaaaaaaaa`. tB `...ab`, `...ac`, extended `...ab`, and the n=4 table (`0x3ffd555555555554` / `...54` / `...56`) all agree |
| 34 | F1 zero-yield DURATION: our code takes the tA route | agrees | our route gives `0x3feaaaaaaaaaaaaa` (redemption time `off + (n-1)`) |
| 35 | F3: three yields collapse to stored base 1.125, the fourth does not | agrees | 1/8, 4/8 (tie to even), 5/8 ulp |
| 36 | F2: zero-coupon PRICE is exactly `Xdiv(R, discount)` | agrees | `bond_core_family.rs:374-387` (coupon sum 0, accrued 0) |
| 37 | Misses at n=1,2,4,8, none at n=3, 5..7 (A5) | data agrees; inference partly wrong | the n=1 misses come from the n=1 closed form (`:741-743`), which uses no exponent (finding 4) |
| 38 | BINOM retained patch 2302/1414 is a net gain with losses | agrees | prompt D.3 and `DnaCalc-Research/design/binomial-inverse.txt` already say one loss each; not a correction of the prompt |
| 39 | E.3 recurrence holds at `1+r` = 2, 4 on 600 ranges and 115 singletons | agrees | `DnaCalc-Research/design/cumulative-payments.txt` section 2 |

## 3. Proposed Excel experiments

Calls are Excel cells. "Ready" means the inputs are fully given and the predictions can be frozen now; "fill" means
something must be chosen first. Bead .6 runs all except where noted.

| ID | Inputs | Calls | Predictions per hypothesis | Status |
|---|---|---|---|---|
| Y1-a | YIELD(44013,44562,0.05,p,100,2,0), p bits `0x4059000000000002`, `0x4058fffffffffffe` | 2 | Y0 `0x3fa999999999997d` / `0x3fa99999999999b6`; Y1=Y2 `0x3fa999999999997e` / `0x3fa99999999999b7` | ready (held-out check of Y2) |
| Y1-b | long: YIELD(44013,46753,0.05,p,100,2,0) at 2 more ulp offsets on the 1/2-ulp denominator edge | 2-4 | compute with `yseed.py` | fill (pick offsets where Y1 != Y2) |
| Y1-c | cohort n=2..20, f=1,2,4, rates {0,0.03125,0.05,0.0575,0.125}, price bits `0x4059000000000000+j`, j in {-4,-2,-1,0,1,2,3,4} | up to 2280; ask only where Y0/Y1/Y2 differ | three frozen seed tables | fill: settlement/maturity per (n, f) must be chosen (coupon date) |
| Y1-d | raw-rate pair: rates `0x3fa9999999999999`, `0x3fa999999999999a`; short tuple at 100+2^-19 and its two neighbours | 6 | rate only via stored coupon: identical pairs; Y2: `...26e978ed/ee`, `...fb/fc`, `...e0/e1` | ready |
| Y2 | short tuple at 100.0000364, 100.00003645, 100.00003646, 100.0000365 and the four mirror prices | 8, then bisection on the price lattice (~2 x 50) | pre-update: first two of each side return the Y2 seed (`0x3fa999910ce29c49`, `0x3fa9999109e0f3e8`, `0x3fa999a22650dc8a`, `0x3fa999a22952851c`), outer two iterate; post-update: displaced ~1e-10 | ready |
| Y3 | (44197,45292,0.025,p,100,1,0), (44013,44562,0.05,p,100,2,0), (44287,44562,0.1,p,100,4,0); p = 100.00003, 100.00006 | 6 | annual 1e-10: seed/seed/iterate then seed/iterate/iterate; periodic: seed x3 then iterate x3. Y2 seeds: `0x3f9999928db6cf0b`, `0x3fa999928db6cf0b`, `0x3fb999928db6cf0b` (100.00003) | ready |
| Y4 | zero coupon (44013,44562,0,p,R,2,0), R in {97,100,128}, p = R(1 +- 2^-10) and R(1 +- 2^-9); coupon controls (50,0.03125),(100,0.0625),(200,0.125) at R(1 +- 2^-24); annual YIELD(44197,45658,0.0325,97+j*2^-20,97,1,4), j in {-16,-4,-1,0,1,4,16} | 12 + 6 + 7 = 25 | general seed + pre-update stop: `0x3f4558e426065664` (low), `0xbf4551c7b40ca88f` (high) for every R at 2^-10; converged root differs by many ulps; at 2^-9 an update is required | ready |
| Y5 | 4 off-coupon tuples x prices {90,97,100,103,110} x adjacent ladders | ~20 x ladder | frozen candidate traces | fill (ladder width, candidate tables); deferred, .13 |
| Y6-a | local replay of R44/R46 answers under each derivative profile | 0 | - | .2 (local) |
| Y6-b | (44197,44927,0,p,R,1,0), (p,R) = (25,100), (80,100) x 2^k, k in {-500,-40,-20,0,20,40,500} | 14 | homogeneous profiles: constant across safe k; cash-sensitive branch: transition | ready |
| Y6-c | off-coupon and odd-n negative-root controls | ? | universal `y > -f` guard: all rejected | fill |
| Y6-d | cap traces at updates 99/100/101 | ? | 100/#NUM vs return-last | .13 (needs a surviving trajectory) |
| I1 | IRR([-1000,1210], g), g bits `0x3fcae147ae147ae1+j`, j=-32..32, flows x 2^-20, 1, 2^20 | 195 | stored-v-only: equal answers within each v0 collision group (bank already violates it twice: m2 vs m1/m3/m4/p1..p4, and p12 vs p16); converted zero exit: common conversion inside each zero group | ready |
| I2 | IRR([-1,2], 1+2^-16), IRR([-2^20,2^21], 1+2^-16) | 2 | P residual (ours): `0x3ff0000000000000` both; vP: ~1+6.2e-11 unscaled, extra update when scaled | ready |
| I3 | IRR([-2^-40, 0, 2^-38], 0) | 1 | initial residual check: 0; OR: ~0.59952; AND (ours): `0x3feffffffffffffc` after 7 updates | ready |
| I4 | IRR([1,-4,4], g), g in {0,-0.5,0.5}, x 2^-20, 1, 2^20 | 9 | ours (cap 200): `0x3feffffeced96e76` (g=0, 31 updates), `0x3fefffff27f72226` (-0.5, 34), `0x3feffffef5453100` (0.5, 29); cap 20: #NUM! | ready; lower value (finding 2) |
| I5-a | [1,-3,2] and [-1,3,-2] at g `0x3fd5555555555555` | 2 | signed seed: next point -61.75 vs 63.25; ours: `0x3fefffffffffffcc` vs `0x3cd8000000000000` (different roots) | ready |
| I5-b | [1,0,-4] at g = -3, -2, -1, nextdown(-1), nextup(-1) | 5 | unrestricted integer power: -3 at g=-3 (ours returns -3); positivity guard: error | ready |
| I5-c | d17/d19 at 11 guesses + 65 adjacent guesses around 2.0, x 3 scales; omitted guess vs 0.1 | ~460 | ours: no number at guess 2 (oscillates; converges only at update 1518 to a different value) | ready |
| Q1-a | both names at (1,.5,.5), (2,.5,.75), (3,.5,.125), (5,.5,.49999999999999994), (5,.5,.5), (5,.5,.50000000000000011) | 12 | leading rule: 1,1,0,3,3,3; complemented tail: 2 on the last two | ready (four of six are bank rows already) |
| Q1-b | T = BINOM.DIST(0,1,.5,TRUE); both names at (1,.5,T) | 3 | strict zero: 1; uniform inclusive: 0 | ready (two stages) |
| Q1-c | at (2,5,.5), (1,5,.5), (2,6,.5): CDF, BETA.DIST companion, both inverses at F and 3 neighbours each side | ~48 | wrong predicate vs wrong CDF bits | ready (two stages); share with dc-6332.8 |
| Q2 | n in {999999,...,1048577} x p in 5 values x k near floor(np) and {0,1,n-2,n-1}; CDF then inverses at nextdown/at/nextup | ~1,300 | first k satisfying the predicate; a bounded walk can break it near endpoints | fill (k window); two stages |
| Q2-dom | (0,.5,.5), (-.5,.5,.5), (1,.5,0), (1,.5,1), p in {0,1,nextup(0),nextdown(1)} | ~16 | validation before truncation: #NUM! at -0.5 | ready |
| L1 | CUMPRINC(.75,1000,pv,1000,1000,0), pv bits `0x3ffc...01/02/03` | 3 | five signatures (all verified) | do not run: all five graphs already refuted by `pvscan-big1.txt` |
| L1-ext | ranges, nper 17 loan, rates 1 and 3, type 1 | ~40 | - | .13 |
| L2 | PMT, PPMT, IPMT, CUMPRINC, CUMIPMT and companions at r=8.0499406029856276e-09, pv=0.05191834857052053 | 10 -> 2 | bank already shows CUMPRINC != -pv and an implied payment equal to ours; remaining: PMT and PPMT directly | ready, reduced |
| L3 | type 1 (0.0013616417420825142,127,220.63364505898258), intervals [1,1],[2,2],[1,2],[2,127],[1,127], both functions; fractional controls (.75,17.5,1.75) | ~18 | COEFFICIENT_FIRST vs PAYMENT_AFTER; CUMIPMT(1,h) = CUMIPMT(2,h) | ready (frozen predictions need the Rust leaves) |
| F1 | DURATION and MDURATION(44256,44562,0,0,2,0); DURATION(44256,44927,0,0,2,0) | 3-4 | `S(off+(n-1))`: `0x3feaaaaaaaaaaaaa`, `0x3ffd555555555554`; `S(n-S(A/E))`: `...ac`, `...54`; `S(S(off+n)-1)`: `...aa`, `...56` | ready (use the corrected 16-digit hex) |
| F1-sched | zero-yield, zero-coupon DURATION/MDURATION on the 16 miss schedules (list in `dur_flag.py` output) | ~70 | time-construction vs power | ready once listed |
| D5 | n = 3,4,7,8,15,16 at settlement fraction 120/180 | 12 | binade at n = 4, 8, 16 only | ready after F1 |
| F2 | PRICE(46463,47859,0,y,R,2,b), y in {0, 0.20055555505885014, 0.20055555505885017}, R in {1,3,7,100,1.0000000000000002,0.99999999999999989}, b in {2,3} | 36 | intersect discount intervals; empty set = not a stored double | ready |
| F3 | zero-coupon PRICE(44256,47119,0,y,R,2,0), 4 yield bits x 6 R | 24 | stored base: first three identical | ready |
| F4 | 8 maturities x yields {2,6,510} x 6 R | 144 | reused fractional factor: exact 2^k scaling | ready |

Total of the ready set, without Q2 and Y1-c: about 1,200 calls.

## 4. Directly replayable candidates, in order of rows reachable

All for dc-6332.2. "Predicted" counts come from exact Python replays unless marked.

1. **DURATION/MDURATION redemption time `S(S(off+n)-1)` (A5, F1, C5 `redemption_T2`).** Reachable: 169 DURATION
   and 140 MDURATION n > 1 misses, all flagged by `S(off+(n-1)) != S(S(off+n)-1)`; at risk: 59 + 54 flagged matched
   rows; 7194 + 714 unflagged rows cannot change. Profiles to replay separately: (a) redemption weight only
   (`excel_x87_div(t_R*100, disc)` with `t_R = S(S(off+n)-1)`, `bond_core_family.rs:754`); (b) (a) plus the
   redemption exponent; (c) all times `S(S(off+k+1)-1)` for weights; (d) (c) plus exponents. Bits need the Rust pow
   chain; not predicted here. 25 DURATION misses have no flagged coupon time, so the redemption term alone must
   carry them.
2. **YIELD seed association Y2 plus par rows through the seed.** In `near_par_seed` (`:590-592`):
   `nc = S(S(S(n*rate)*100)/f)`; `den = S(S(n*R + S(S(d*n)/2)) + S(d/2))`; `seed = S(f*S(S(nc-d)/den))`; and drop the
   par shortcut (`:640-647`) so price-100 rows take the seed. Predicted: +31, 0 losses: 26 short, long-005, r18 003,
   011, 020, 021 (YIELD 66 -> 97). Rows 024 and 026 stay. Plain binary64 and x87 stores give the same bits.
3. **IRR converted zero exit (C2, first half only).** `cashflow_rate_family.rs:494`: return
   `sub(div(1.0, before), 1.0)` instead of `guess`. Predicted: +16, 0 losses (irrB-lad m4, m3, m1, p1-p4; oc50-52,
   oc54-57; d03-root; d05-root); IRR 533 -> 549. m2 (and oc53) stay wrong, as the collision argument requires.
   Keep cap 200. Do not add the raw-guess residual (section 5).
4. **BINOM.INV/CRITBINOM predicate (A3, C3).** Port `DnaCalc-Research/evidence/ld1/binom-inv.patch` by hand (or
   C3's lower-bound search with the current `binom_cumulative`). Measured: BINOM.INV 2290 -> 2302, CRITBINOM
   1402 -> 1414, losing BINOM.INV(5,0.5,0.5) and its CRITBINOM twin. Six remain per function, waiting on three
   BINOM.DIST leaves (beta family).
5. **DURATION/MDURATION n = 1 through the general loop (not in the answer).** Remove the `p.n == 1` shortcut
   (`:741-743`) and let the k = 0 term plus redemption run. Reachable: 8 DURATION, 7 MDURATION misses; 22 + 23
   matched n = 1 rows at risk. (proxy) 25 of 30 DURATION n = 1 rows match with a correctly rounded power; the answer
   is sensitive to the power's last bit, so the Rust replay with `excel_pow_chain` decides.
6. **CUMPRINC type 0 backward recurrence R0 (C4, E.3).** Reachable: 200 type-0 misses (93 of 293 match). E.3 says
   it is within 3 ulp on ranges where the current code is up to 16, so the exact gain is unknown; it is the identified
   shape at power-of-two rates and the base for any later singleton fix. CUMIPMT's tiny-rate misses follow CUMPRINC
   (finding 6).
7. **CUMPRINC type 1 finishes (C4).** COEFFICIENT_FIRST and PAYMENT_AFTER as separate profiles on the 156 type-1
   misses; the earlier due loop lost 28 matched rows, so list losses.
8. **YIELD stopped iteration for on-coupon non-par designs (C1).** Designs oncoupon-b4-f1 (54 rows, A = 0, R = 97):
   general-R seed, pre-update 1e-10 stop, CAP 100, with analytic, forward, backward and central slopes, objective
   `pcomp` and the PRICE graph. Real-arithmetic lead (`ybias.py`): from the prompt's best general seed, analytic Newton
   leaves biases 30-300 times smaller than Excel's, forward differences give the wrong sign, and a backward difference
   with h = 1e-7 gives the right sign and order on all five bias designs (b4-f1: -1.23e-15 vs Excel -2.19e-15; b0-f4
   with h = f*1e-7: -1.46e-14 vs -1.9e-14). Not a bit prediction.
9. **PRICE.** No candidate in the answer reaches its 50 misses: 38 are unflagged by the binade test, and F2/F3 need
   Excel answers first.

Not to replay: the raw-guess IRR residual, CUMPRINC R1a/R1b/R2 and the payment-first control, IRR CAP=20.

## 5. Wrong, overstated, inconsistent, or in conflict with earlier rounds

1. **The raw-guess residual is presented as "a particularly small candidate that preserves this information" (A2,
   C2).** It does carry the information into F0, but as an executable candidate it is refuted: on irrB-lad-m2 it
   returns `0x3fcae147ae147ae0`, not Excel's `...ae8`; on p4 it returns `...ae8` where Excel and the converted zero
   exit give `...ae0`; bank 533 -> 530; Fable 5.2's fresh I7 two-flow set 149 -> 139 (I1 multi-flow 88 -> 90). Its
   companion statement "retaining an extended factor ... is an alternative" also fails for this pair: `S(E(1/E(1+g)))`
   collapses both guesses to the same v0.
2. **L1's "leading normalized-quotient candidate" was already dead.** The answer rightly asked for a replay of E.3's
   2,000-pv certificate first; done here, every graph in its table fails (0, 20, 0, 0, 163 of 2000). The bias of
   Excel's value against the exact `pv*3/7` is 1 or 2 ulp toward zero on every one of the 2000 pv, which no single
   extra money rounding with the true constants produces. Agrees with E.3 ("best cover 82%").
3. **A4's one-period bound argument rests on a premise the bank refutes** (finding 6). The prompt's E.2 claimed the
   principal is -pv; the bank row r15-cumprinc-036 shows it is not, and the answer did not check. L2's four-way
   interpretation table is mostly answered.
4. **"No exact gain count follows" (C1 expected movement) is overcautious.** The seed rows are pure binary64
   arithmetic; the count is exactly +31 for Y2 (+30 for Y1).
5. **The n = 1 DURATION misses are cited as support for the binade association (A5).** Our n = 1 path computes no
   time sum (`bond_core_family.rs:741-743`); its misses come from a yield-independent shortcut (finding 4). The
   association is still strongly supported by the n > 1 rows (finding 3).
6. **Hex typos in F1**: `0x3ffaaaaaaaaaaaa` and `0x3feaaaaaaaaaaaa` have 15 digits; the intended values are
   `0x3ffaaaaaaaaaaaaa` and `0x3feaaaaaaaaaaaaa`.
7. **The F.2 metadata catch is understated.** All A/E columns of the prompt's F.2 tables are calendar spans, not
   the basis day counts the code uses (`gen_bond_tables.py`); any reconstruction from those tables must recompute A
   and E with `dd`/`days_between_num` and `dc`.
8. **"Neither a literal 20-update cap nor the current 200-update cap is measured" (A2).** Conditionally it is
   bounded: three matched bank rows (d07, d05, d06 at guess 2.0 "far_p200") need 23, 43 and 52 updates under our
   trajectory, so a 20 cap would lose them. Not a measurement of the cap, but it lowers I4's value.
9. **d17/d19 (A2, I5).** Our model at guess 2.0 does not run out of steps quietly: the signed second point throws v
   into an oscillation between ~0.336 and ~13; with no cap it settles only at update 1518, on
   `0x3fa3670ea52224e0`, not Excel's `...7340`. Excel's trajectory differs early (second point, safeguard or restart),
   which supports I5-c over a cap explanation.
10. **BINOM (A3).** The answer frames "net gain, not twelve fixes" as a correction; the prompt and
    `binomial-inverse.txt` already said one loss each. R41 also matched the public-CDF predicate on 6,270 fresh
    inverse/alias selections, which makes Q1-a low value.
11. **Earlier rounds that overlap.** R35's "48 subtraction labels" for DURATION (work-report-35) predate R37's
    arithmetic fix and their list is not in the local research checkout; check whether `S(S(off+n)-1)` was among them
    before treating candidate 1 as new. R38's 22 refuted power profiles concern the power, not the time construction,
    so they do not conflict. R49's "no common seed under analytic Newton" agrees with the backward-difference lead in
    candidate 8.
12. **Prompt inconsistency (not the answer's).** C.2 says the family B ladder is centred on `0x3fcae147ae147ae1`; the
    bank labels are centred on `...ae0` (m1 = `...adf`, m2 = `...ade`).

## Top five actions by expected value

1. Replay the DURATION/MDURATION redemption-time association `S(S(off+n)-1)`, profiles (a)-(d), plus the n = 1
   general loop: up to 177 + 147 rows reachable. **dc-6332.2**
2. Land YIELD Y2 with par rows through the seed: +31 rows exactly, no loss (66 -> 97). **dc-6332.2**
3. Land the IRR converted zero exit alone: +16 rows, no loss (533 -> 549); keep cap 200, drop the raw-guess residual.
   **dc-6332.2**
4. Port the BINOM.INV/CRITBINOM research patch: +12 net in each function. **dc-6332.2**
5. First solver Excel round from the ready set (about 1,200 calls): F1 + F1-sched + D5 (confirms action 1),
   Y1-a/Y1-d/Y2/Y3/Y4 (seed and stop units), I1/I2/I3/I5, L2 reduced to 2 calls, L3, F2/F3. Skip L1. **dc-6332.6**
