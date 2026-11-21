# The last bit: Excel's forward and inverse gamma family

*Researched in Carbon rounds `cb6`–`cb8` and landed in Neon rounds `nx5`–`nx7` (2026-11-21), closing beads `dc-j9k.60`, `dc-j9k.63`, and `dc-j9k.64`.*

The gamma distribution family spans twelve closely wired worksheet functions: seven forward functions (`POISSON`, `POISSON.DIST`, `GAMMADIST`, `GAMMA.DIST`, `CHIDIST`, `CHISQ.DIST`, `CHISQ.DIST.RT`) and five inverse functions (`GAMMA.INV`, `GAMMAINV`, `CHIINV`, `CHISQ.INV`, `CHISQ.INV.RT`). They share one forward incomplete-gamma kernel (`gratio(a, x)`), one prefactor (`poisson_mass(a, x)`), and one inverse solver.

As with `NORMSINV` ([last-bit-normsinv.md](last-bit-normsinv.md)), everything below was inferred strictly from recorded and live Excel answers (build `16.0.20527.20196` via Touchstone) and public numerical-analysis literature—no Microsoft code was read or disassembled.

## Where we started

Before `nx5` (`ade42c3f` / `7d2194b7`), an earlier fix (`6bbee487`, `dc-ymjf.2.27`) had already landed Temme's uniform asymptotic region (`0.7 < x / a < 1.3`), eliminating 94-ULP errors on forward `GAMMADIST`. Across the twelve functions, however, `3,584` of `162,739` witness-bank rows still missed:

- **Seven forward functions:** `141,443` of `142,579` matched (`1,136` misses, `65` of at least `5` ULP), concentrated in `POISSON` (`340` misses), `POISSON.DIST` (`552` misses), `GAMMADIST` (`96` misses), `CHIDIST` (`43` misses, worst `13` ULP), `CHISQ.DIST` (`46` misses), `CHISQ.DIST.RT` (`30` misses), and `GAMMA.DIST` (`29` misses).
- **Five inverse functions:** `17,712` of `20,160` matched (`2,448` misses, `757` of at least `5` ULP, worst `530` ULP on `CHIINV`). Our old inverse used bisection, a `2.7e10` post-hoc multiplier, and a `113`-bit double-double polish that disagreed with Excel's actual solver plateaus by hundreds of ULPs.

Over three paired Carbon/Neon rounds, those `3,584` misses fell to `784` (`159,155 -> 161,955 / 162,739`, net `+2,800`), with `POISSON` (`1,556 / 1,560`, worst `1` ULP), `POISSON.DIST` (`49,707 / 49,710` decoded, `0` cumulative CF/asymptotic/Temme misses), `CHIDIST` (`3,241 / 3,241` exact), `CHISQ.DIST.RT` (`2,858 / 2,859`, worst `2` ULP), `CHIINV` (`4,290 / 4,293`, worst `2` ULP), `CHISQ.INV` (`2,900 / 2,903`, worst `2` ULP), and `CHISQ.INV.RT` (`4,190 / 4,191`, worst `2` ULP) all exact or within `2` ULP on the witness bank.

## Stage 1 (`cb6` / `nx5`, `dc-j9k.60`): the inverse program and `alpha == 0.5`

Carbon `cb6` and Neon `nx5` replaced our ad-hoc inverse polish and bisection with Excel's real unit solver. Every intermediate arithmetic operation runs under x87 `PC = 64`, round-to-nearest-even (`CW = 0x133F`), with `st(...)` denoting a store to `binary64` and `flush(v)` flushing nonzero subnormals (`|v| < 2^-1022`) to `0.0`:

1. **Exact `alpha == 0.5` / `df == 1` closed form (`erfinv(p)^2`):** Excel routes `alpha == 0.5` directly to the exact same 4-piece `erfinv` / AS 241 rational tables already proven for `NORM.S.INV` (`PA/QA`, `PB/QB`, `PC/QC`, `C/D`, `E/F`), omitting `NORM.S.INV`'s `sqrt(2)` factor on the three central pieces and multiplying the AS 241 tail by `RN53(1 / sqrt(2))` (`0x3fe6a09e667f3bcd`). The unit solver returns `flush(st(y * y))` before multiplying by `beta` (or `2.0` for `df == 1`).
2. **AS 91 start and unfactored damped Halley loop:** For general shapes (`alpha != 0.5, 1.0`), lower-tail calls follow Best & Roberts (AS 91, `-1.24 * ln(p)` power-law start vs Wilson-Hilferty), while right-tail chi-square (`CHIINV`, `CHISQ.INV.RT` for `df >= 3`) passes `z = -NORM.S.INV(q)` into Wilson-Hilferty so tiny `q` down to `1e-100` never collapses `1 - q` to `1.0`. The solver runs an unfactored Halley step `2 * df / (2 * pdf - df * ((a - 1) / x - 1))`, damped on sign changes by Cran–Martin–Thomas (AS 109) factor-of-3 step halving (`step / 3.0`, skipping the trial counter while dividing down), up to `106` trials, publishing `flush(best_x)` chosen by strict `<` on `|df|`.
3. **Strict `prob > 0.9999` (`0x3fefff2e48e8a71e`) complement residual switch:** Single-bit bisection on both `GAMMA.INV` and `CHIINV` located an exact switch at `prob > 0.9999`. Above `0.9999`, the residual flips to the upper/lower complement (`comp(prob) - flush(Q)` or `flush(P) - comp(prob)` where `comp(u) = st(0.5 + st(0.5 - u))`), eliminating catastrophic cancellation when `1 - p < 1e-12` (`max 9.3e13 ULP -> 6 ULP`).
4. **High-shape Goldstein (`alpha >= 2.5e10`):** Restored the unconditional stored Goldstein cube `st(alpha * st(st(c * c) * c))` for `[2.5e10, 2^95)` and kept the `[2^115, 2^600)` large-shape route.

**Stage 1 evidence (`cb6` + `nx5`):**
- **Prospective frozen holdout (`cb6` `frozen_predictions.jsonl`, `500` unseen disagreement rows, SHA-256 `ac6e3cea3279aac4004a3b349d2a1c432539490159e63f8325cb7655aca028bd`):** `291 / 500` (`58.2%`, `434 / 500 <= 1` ULP) vs `130 / 500` (`26.0%`) on `origin/master`, including `140 / 140` (`100.0%`, `max 0` ULP) on `a == 0.5` / `df == 1`.
- **Inverse witness banks (`nx5`, `48207c4d`):** `17,712 -> 18,844 / 20,160` (`+1,132` net: `+1,327 / -195`), `>= 5` ULP misses `757 -> 373`, worst error `530 -> 166` ULP; pinned inverse banks `323 -> 348 / 360` (`+25 / -0`, zero pinned losses; `CHIINV` `109 -> 118/120`, `CHISQ.INV` `105 -> 118/120`).

## Stage 2 (`cb7` / `nx6`, `dc-j9k.63`): forward `gratio` map and Goldstein 451

Our forward incomplete-gamma kernel was based on DiDonato & Morris's NSWC `gratio` (ACM TOMS 708), which switches to an asymptotic series at `x0 = 31.0`, switches between series and continued fraction near `ln(10)` for `1 < a < ln(10)`, and computes half-integer finite sums by a recurrence with `exp(-x) / (sqrt(pi) * sqrt(x))`.

Live Excel probes proved Excel's `gratio(a, x)` removes all three differences:

1. **Unified finite-sum domain `a < x <= 30.0` and stored half-integer Horner:** For integer `a in 2..=20` and half-integer `a in 1.5..=19.5`, Excel runs the finite sum strictly on `a < x <= 30.0` (at `x == a`, live probes matched `42 / 42` on the lower Taylor series). For half-integers, Excel evaluates a stored x87 Horner polynomial over `RN53(2^(k+1) / (2k+1)!!)` multiplied by `excel_exp(st(-x - st(0.5 * excel_log(x)))) * (1 / sqrt(pi)) * x`, then added to `erfc_precise_kernel(sqrt(x))`. Dividing `POISSON.DIST` by adjacent mass exposed the `-17/16` derivative slope of `-x - 0.5 * ln(x)` in a single pass.
2. **No `ln(10)` or `x0 = 31.0` switches for `1 < a <= 20`:** Adjacent-double sweeps proved `x <= a` runs the forward Taylor series and `x > a` enters the continued fraction immediately—and stays on the continued fraction past `x = 31.0` all the way down near `DBL_MIN`.
3. **Forward x87-stored upper asymptotic for `a > 20, x / a >= 1.3`:** Instead of NSWC's backward `wk[1..20]` recurrence, Excel runs a forward loop (`amn = st(amn - 1.0); nt = st(t * st(amn / x));` stopping when `|nt| > |t|` or `|t| / s <= 2^-52`) and multiplies `st(pm * st(st(a / x) * s))` using `poisson_mass(a, x)` (Loader's `exp(-stirling_error(a) - deviance(a, x)) / sqrt(2 * pi * a)`).
4. **Goldstein (1973, CACM Algorithm 451) start for `a > 1.0` inverses:** While investigating why `nx5`/`cb7` still had plateau misses at `df = 10..100` and `a > 20`, Neon `nx6` bisected `a = 0.999999` vs `a = 1.000001` and ran sign-flip ablations over all 21 coefficients (`195..269` lost matches per flip). That proved Excel uses Goldstein's full 7-term stored polynomial (`f = st(1 / (2*a))`, `u = st(z * sqrt(f))`, `ck = st(A[k] + st(f * st(B[k] + st(f * C[k]))))`, `x0 = st(a * st(st(c * c) * c))`) whenever `a > 1.0`, keeping Wilson-Hilferty only for `a < 1.0`.

**Stage 2 evidence (`cb7` + `nx6`):**
- **Frozen holdouts:** `cb7` `frozen_cb7_predictions.jsonl` (`800` unseen rows, SHA-256 `d4c28377c9186756018d90412e7bae57a0c4e23f73f22e9378d862bd34d8cee4`) scored `461 / 800` (`57.6%`) vs `79 / 800` (`9.9%`), including `318 / 320` on `a > 20, x/a >= 1.3` (`240 / 240` for `y >= 31`). `nx6` `frozen_predictions_nx6.jsonl` (`300` unseen rows, SHA-256 `85ce39074c70505817aff7e3931b0e67904e49a5c86d94b81f888fd565319cfd`) scored **`252 / 300` (`84.0%`)** vs `27 / 300` (`9.0%`), with `60 / 60` (`0` ULP) on half-integer forward, `40 / 40` (`0` ULP) on asymptotic forward, and `20 / 20` on `a > 20` `GAMMA.INV`.
- **Full & pinned witness banks (`nx6`, `88b88617`, 12 functions):** `160,287 -> 161,032 / 162,739` (`+745` net: `+824 / -79`, `>= 5` ULP `438 -> 383`). Pinned banks rose `714 -> 721 / 732` (`CHIDIST` `117 -> 120 / 120` exact, `CHISQ.INV` `118 -> 120 / 120` exact).

## Stage 3 (`cb8` / `nx7`, `dc-j9k.64`): Modified-Lentz CF and Halley's exact stopping rule

After Stage 2, nearly all remaining `POISSON` (`141` misses), `POISSON.DIST` (`370` misses), and `a < 1, x >= 1.1` misses sat inside the upper-tail continued fraction `gratio_cf`. By dividing live `POISSON.DIST(k - 1, m, TRUE)` by `POISSON.DIST(k, m, FALSE)` at identical `m`, Carbon cancelled the `500`-ULP saddle-point exponent grid and exposed the continued-fraction inner loop directly.

1. **Thompson–Barnett Modified Lentz (`gratio_cf`) with `TINY = 1e-30`:** For all `a <= 20` upper-tail calls (`a < 1, x >= 1.1` and `1 < a <= 20, x > a` outside finite sums), Excel evaluates the **uncontracted Legendre continued fraction** using Modified Lentz starting from `b_0 = 0` (`f = c = TINY`, `d = 0.0`) with *Numerical Recipes* `FPMIN = 1.0e-30` (`0x39b08b2a2c280291`), storing every operation to `binary64` at x87 PC64, stopping at `k > 0 && del == 1.0`, and returning `st(pm * f)` directly (without factoring `a` out into `pm * a`). Changing `TINY` to `1e-50` or `1e-100` breaks nearly half of all `10,000` test rows because step `k = 0` computes `st(x + st(a / TINY))` in `binary64`. (`excel_gamma_fraction` was left untouched for `BETA.DIST` / `BETADIST`.)
2. **Inclusive `<= 1.1` threshold in `poisson_mass` (`nx7`):** Single-bit bisection at `1.1` (`0x3ff199999999999a`) vs `next_up(1.1)` proved `x <= 1.1 || lambda <= 1.1` (not `< 1.1`), resolving `GAMMA.DIST` PDF misses in `(1, 1.1]` and `GAMMA.INV` misses near `a = 1 + 2^-52`.
3. **`a < 1.0` Wilson-Hilferty cube `a * exp(3 * ln(c))` (`nx7`):** On `a < 1.0`, Excel computes the Wilson-Hilferty cube via `excel_x87_mul(a, excel_exp(excel_x87_mul(3.0, excel_log(c))))`, not `a * (c * (c * c))`.
4. **Exact Halley stopping and damping stop (`nx7`):** Excel stops on `|next - x| <= f64::EPSILON * x` returning `best_x` without evaluating `next` (eliminating `cb6`'s extra `x_cand` step), and stops immediately when a damped step hits `step_limit` without improving `best_err` (`last_adj.abs() >= step_limit && !improved`).

**Stage 3 evidence (`cb8` + `nx7`):**
- **Frozen holdouts:** `cb8` `frozen_cb8_predictions.jsonl` (`640` unseen rows, SHA-256 `bd55c71f7e375c48957caa593ab0cdfff45ebcf9ab874622eeb42c6a79718503`) matched `599 / 640` (`93.6%`), including **`560 / 560` (`100.0%`, `max 0` ULP)** across all six forward functions (`POISSON`, `POISSON.DIST`, `CHIDIST`, `CHISQ.DIST.RT`, `GAMMA.DIST`, `GAMMADIST`). `nx7` Proof 1 (`fb2f7609eec46828812b4fcbd9cefde80afccf1ed6672350d8781911482d571c`) scored `57 / 60` (`a * exp(3 * ln(c))` + stopping rule) vs `1 / 60` (`c^3`); `nx7` Proof 2 (`e727a10fd112e427490480afd2dc14a60695e94dde0d94da67b9e64192a5919c`) scored `30 / 30` (`pm <= 1.1`), `20 / 20` (`1.1` boundary), `30 / 30` (`|next - x| <= eps * x` vs `x_cand`), `30 / 30` (`|next - x|` vs `|last_adj|`), `38 / 50` (damping stop), and `13 / 15` (`hit` vs `sign_flip`).
- **Full & pinned witness banks (`nx7`, `de7cbede`, 12 functions):** `161,032 -> 161,955 / 162,739` (`+923` net: `+934 / -11`; `1..4` ULP misses cut `1,320 -> 454`, `>= 5` ULP `383 -> 326`). Pinned banks reached `724 / 732` (`6` of `8` pinned banks at `100%` exact: `POISSON.DIST`, `GAMMADIST`, `CHIDIST`, `CHISQ.DIST.RT`, `CHIINV`, `CHISQ.INV`).

## Cumulative witness-bank summary (`nx5`–`nx7`)

| Function | Rows | Before `nx5` | After `nx7` | Net | `>= 5` ULP | Worst ULP | Class (`Before -> After`) |
|:---|---:|---:|---:|---:|---:|---:|:---|
| `POISSON` | 1,560 | 1,220 | **1,556** | +336 | `24 -> 0` | `7 -> 1` | `numeric -> last_bit` |
| `POISSON.DIST`* | 49,714 | 49,162 | **49,707** | +545 | `3 -> 0` | `7 -> 1` | `numeric -> last_bit` |
| `GAMMADIST` | 2,954 | 2,858 | **2,874** | +16 | `14 -> 12` | `8 -> 8` | `numeric` |
| `GAMMA.DIST` | 77,975 | 77,946 | **77,959** | +13 | `4 -> 1` | `8 -> 5` | `numeric` |
| `CHIDIST` | 3,241 | 3,198 | **3,241** | +43 | `9 -> 0` | `13 -> -` | `numeric -> exact` |
| `CHISQ.DIST` | 4,276 | 4,230 | **4,230** | +0 | `6 -> 6` | `9 -> 9` | `numeric` |
| `CHISQ.DIST.RT` | 2,859 | 2,829 | **2,858** | +29 | `5 -> 0` | `6 -> 2` | `numeric -> last_bit` |
| `GAMMA.INV` | 4,484 | 3,831 | **4,166** | +335 | `212 -> 155` | `347 -> 166` | `numeric` |
| `GAMMAINV` | 4,289 | 3,731 | **3,984** | +253 | `206 -> 152` | `264 -> 232` | `numeric` |
| `CHIINV` | 4,293 | 3,831 | **4,290** | +459 | `141 -> 0` | `530 -> 2` | `numeric -> last_bit` |
| `CHISQ.INV` | 2,903 | 2,566 | **2,900** | +334 | `73 -> 0` | `165 -> 2` | `numeric -> last_bit` |
| `CHISQ.INV.RT` | 4,191 | 3,753 | **4,190** | +437 | `125 -> 0` | `116 -> 2` | `numeric -> last_bit` |
| **Total (12 fns)** | **162,739** | **159,155** | **161,955** | **+2,800** | **`822 -> 326`** | — | **misses `3,584 -> 784`** |

*\*`POISSON.DIST` includes `4` excluded `input-changed` rows (`round3-tiny-publication--4..-1`); only `3` decoded `1`-ULP rows remain in `49,710` rows (`0` cumulative CF/asymptotic/Temme misses).*

## Method lessons

- **Cancel a noisy shared prefactor with an adjacent Excel oracle:** Dividing `POISSON.DIST(k - 1, m, TRUE)` by `POISSON.DIST(k, m, FALSE)` at identical `m` cancels the `500`-ULP saddle-point exponent grid completely, turning half-ULP inner-loop store sites (`pm * st((a/x) * s)`), `-x - 0.5 ln x` slopes, and `TINY = 1e-30` CF artifacts into direct readings.
- **Abbreviating constants hides real structure; ablate the start before blaming the step:** `nx6` tested alternative root steps and thresholds before bisecting `a = 0.999999` vs `1.000001` and discovering Goldstein Algorithm 451.
- **Avoid power-of-two blind spots:** Factoring `a_0 = 1` out of `gratio_cf` matched `2,000 / 2,000` at `a = 2` (exact binary scaling) while failing `60%` of general shapes; targeted 1-in-`3e5` tie-breakers separated `del == 1.0` from `f * del == f`.
- **Distinguish forward error from solver error:** Fixing `gratio` shifted `cb6-holdout-076` by `1` ULP because an old `-1` ULP `wk[]` error had accidentally cancelled a solver residual.
- **Even-LSB signatures mark `0.5 + (0.5 + l)`:** Observing `64 / 64` consecutive outputs with `LSB == 0` (`p = 2^-64`) directly pinpoints Morris's `st200` complement domain (`x^a >= 0.5`).

## What stays open

- **`dc-j9k.59` (`a < 1, x < 1.1` leaf and remaining inverse residuals):** `73` `GAMMADIST`, `11` `GAMMA.DIST`, `46` `CHISQ.DIST` (`a = 0.5` / `gratio_series`) misses, and nearly all remaining `>= 5` ULP `GAMMA.INV` (`155` rows, worst `166` ULP) / `GAMMAINV` (`152` rows, worst `232` ULP) misses magnified by `1 / pdf`. `cb8` proved the `x < 1.1` region splits at `x = 0.25` (with Morris's `gratio_gam1` `-2` ULP seam at `a = 0.5` appearing for `x > 0.25` and `st200` running above `x^a >= 0.5`), leaving the `(x, a)` power leaf below `x^a < 0.5` to isolate. Also open under `dc-j9k.59`: `7` remaining `1`–`2` ULP chi-square inverse misses (from `1`-ULP `gratio_series`/`gratio_after40` forward residuals) and the ultra-tiny `q <= 1e-44, df <= 6` `#NUM!` corner (`CHIINV(1e-42, 6)` succeeds while `CHIINV(1e-44, 6)` returns `#NUM!`).
- **`dc-j9k.42` (`POISSON` / `POISSON.DIST`):** `4` remaining `1`-ULP `POISSON` Temme rows (`a > 48`) and `3` remaining `1`-ULP build-20131 `POISSON.DIST` rows (`oc26382`, `oc30176`, `oc33238`) plus `4` excluded `input-changed` rows.
- **`dc-ymjf.2.27`:** Tracks the overall forward gamma-distribution witness remainder (`154` misses across `142,579` rows).
