# The last bit: Excel's incomplete-beta family

*Mapped in `docs/research/beta-family-2026-11-12.md`, researched in Carbon rounds `cb14` and `cb18`, and landed in Neon/Windows rounds `nx12`, `xl2`, and `nx17` (2026-11-22), commits `a423c5c0d`, `8ac7c9e8d`, `b0c75eb2e`, and `9b35d0fce`.*

Twenty-nine worksheet functions in Excel share one mathematical engine underneath: the regularized incomplete beta function `I_x(a, b)` and its inverse. That single substrate feeds not only `BETA.DIST`, `BETADIST`, `BETA.INV`, and `BETAINV`, but also the binomial and negative-binomial distributions (`BINOM.DIST`, `BINOMDIST`, `BINOM.DIST.RANGE`, `BINOM.INV`, `CRITBINOM`, `NEGBINOM.DIST`, `NEGBINOMDIST`), the F family (`F.DIST`, `F.DIST.RT`, `FDIST`, `F.INV`, `F.INV.RT`, `FINV`, `F.TEST`, `FTEST`), and the Student-t family (`T.DIST`, `T.DIST.RT`, `T.DIST.2T`, `TDIST`, `T.INV`, `T.INV.2T`, `TINV`, `T.TEST`, `TTEST`, `CONFIDENCE.T`). Fix one arithmetic detail inside the incomplete-beta kernel and a dozen seemingly unrelated statistical functions move in lockstep.

As with `NORMSINV` ([last-bit-normsinv.md](last-bit-normsinv.md)) and the gamma family ([last-bit-gamma.md](last-bit-gamma.md)), everything below was inferred strictly from recorded and live Excel answers (build `16.0.20527.20196` via Touchstone) and public numerical-analysis literature—primarily DiDonato & Morris (1992, ACM TOMS 708) and Cran, Martin & Thomas (1977, AS 109). No Microsoft source code was read, disassembled, or decompiled.

## Where we started

An earlier structural survey (`docs/research/beta-family-2026-11-12.md`, `dc-6332.8`, `264,002` Excel calls) had already established the map of Excel's incomplete-beta kernel (`bratio`). Excel dispatches inputs into the same broad regions as DiDonato & Morris's TOMS 708 `bratio`—a power series (`bpser`) when `b * x <= 0.7`, a finite upward recurrence (`bup`) for moderate shapes, a continued fraction (`bfrac`) for interior large shapes, and a gamma-based asymptotic expansion (`bgrat`) when one shape is large and the other is small—but the bodies of those arms are not TOMS 708's code. Instead, Excel builds them from pieces it shares with `GAMMALN`, `BINOM.DIST`, and `POISSON.DIST` (for instance, `bpser` sums from `1 / a` with a compensated exponential `EXP(hi) * s * (1 + lo)`, `BINOM.DIST` reads the complement output `1 - I_p(k + 1, n - k)`, and `FDIST` passes `1 / (1 + r)` and `r / (1 + r)` with `r >= 1`). Those first two commits lifted the bank by `+3,225` and `+934` rows.

By the morning of November 22, however, **`20,467` of `192,972` rows** across the 29 incomplete-beta callers still missed Excel (`172,505 / 192,972` exact), and our code had accumulated patchwork around the hard corners:

- **Five hard-coded `#NUM!` bounding boxes** in `distributions.rs` (`beta_inverse`) trying to fence off regions where Excel returned `#NUM!`.
- **A tiny-second-shape patch** in `beta_dist_kernel` hard-coding `1e-30 * exp(-t)` for `0.1 <= a <= 15`, `b <= 2^-180`, and `[0, 1]` bounds.
- **Inherited TOMS 708 inner loops:** a Wallis forward recurrence for `bfrac` that missed Excel by `2..8` ULP even on clean dyadic inputs, and a `binary64` `bgrat` that threw away `11` to `27` bits of precision when subtracting `b` from `(a + b - 0.5) * ln(1 + b/a)`.

Over one day of paired research and landing (`cb14`/`nx12`, `dc-i2wk3`, `dc-iuwct.51`, and `cb18`/`nx17`), exact bit matches across all 29 callers rose from **`172,505` to `182,814` of `192,972`** (**`+10,309` net exact rows**, cutting total misses nearly in half from `20,467` to `10,158`), with zero functions regressed at any step.

## Stage 1 (`cb14` / `nx12`, commit `a423c5c0d`): deleting six patches (`+1,220 / -0`)

A hard-coded bounding box in a numerical solver is usually a symptom: it means you have not yet found the real stopping rule or overflow check inside the algorithm. Carbon `cb14` replaced six such patches with four simple rules that belong to Excel's actual solver and `bratio` dispatcher. Every named step `st(...)` runs under x87 64-bit significand precision (`PC = 64`, round-to-nearest-even, `CW = 0x133F`) and rounds back to `binary64` on store:

1. **No `gamma_limit` diversion in the inverse loop (`+1,057 / -0` bank rows):** Previously, when `b + a == b` in `binary64`, our `beta_newton_lower` diverted onto a gamma-limit approximation. Excel does not divert: its AS 109 Newton loop evaluates `beta_dist_kernel` (`bratio_bpser` and `beta_density_at`) directly even when `b` swallows `a` (`40 / 40` exact on unseen `h1-glim`).
2. **One `0.5` overflow start, subnormal derivative zeroing, and a `700`-trial publication threshold:**
   - In `beta_inverse_start`, whenever `base <= 0` overflows either `base^3` or `x = 1 - exp(t)`, Excel starts at the midpoint `0.5` (`12 / 12` on unseen `h2-mid`).
   - Inside `beta_newton_lower`, when `a < 1 || b < 1`, Excel stops and publishes `best_x` if `x * (1 - x) * density` flushes from a subnormal (`< 2^-1022`) to `0.0`—explaining why Excel publishes `0.5` at `a = 365` even while public `BETA.DIST(0.5, 365, b, FALSE)` is still normal (`8 / 8` on `ww2/dc-ymjf.2.38.2`).
   - When the Newton loop exhausts `MAX_TRIALS = 700`, Excel publishes `best_x` if and only if `best_err <= 1e-12 * p` (using the reflected `p <= 0.5`), returning `#NUM!` otherwise—the same relative tolerance already seen in the gamma inverse solver.
   - Together, those mechanics reproduce every `#NUM!` region naturally, letting `a423c5c0d` **delete all five hard-coded bounding boxes** in `distributions.rs` (`24 / 24` on unseen `h4`, `149 / 149` in `f1_stats.rs`).
3. **Subnormal `LowerPower` seed split (`xb = flush_subnormal(x0 * b0)`):** When `max(a0, b0) > 1`, `b0 <= 1` goes straight to `bpser`, while `b0 > 1` with `x0 < 0.1` forms `xb = flush_subnormal(st(x0 * b0))` before testing `exp(a0 * ln(xb)) <= 0.7`. If `xb` flushes to `0.0`, `bratio` returns `NaN` (`#NUM!`).
4. **No `fpser` and no `apser` in `bratio` (`+163 / -0` bank rows):** Removing TOMS 708's `bratio_fpser`, `bratio_apser`, and dead `bratio_psi` lets `bpser` / `bup` / `bgrat` reproduce Excel's tiny-shape answers across all `[lower, upper]` bounds, allowing `a423c5c0d` to **delete the tiny-second-shape `1e-30 * exp(-t)` patch** in `beta_dist_kernel` (`39 / 40` exact, `40 / 40 <= 2` ULP on unseen `h6`).

### Single probes that separated the rivals in `cb14`

- **One-bit cliff at `b = 1` vs `b = 1 + 2^-52` (`LowerPower`):** Call `BETA.INV(0.49, 0.001, b)`. At `b = 1.0` (`0x3ff0000000000000`), `b0 <= 1` sends the subnormal initial seed `x0` straight into `bpser`, and Excel returns `0.0`. Increase `b` by a single ULP to `1 + 2^-52` (`0x3ff0000000000001`), and `bratio` enters the `b0 > 1, x0 < 0.1` branch, flushes `x0 * b0` to `0.0`, and flips immediately to `#NUM!`. Yet at `b = 2.5` (`p = 0.475478035806142`, `a = 0.00105`), `x0` is still subnormal while `x0 * 2.5` crosses back above `DBL_MIN` (`2^-1022`), so `xb` survives and Excel returns `0.0` again (`54 / 54` across `21 / 21` `probe1` and `33 / 33` unseen `LowerPower` holdout).
- **The `1e-12 * p` knife-edge at `700` trials:** At symmetric tiny shapes `a = b = 3.3e-85` (`0x2e6483bbbe8fbccf`), `BETA.INV(0.499999999999515, a, b)` leaves a residual of `4.85e-13 <= 1e-12 * 0.5` after `700` trials and returns `0.5`; nudging `p` to `0.499999999999485` pushes the residual to `5.15e-13 > 5e-13` and flips Excel to `#NUM!` (`48 / 48` on unseen `h3_exhaustion_700_edge`).

**Stage 1 evidence (`cb14` / `nx12`, `a423c5c0d`, checked):**
- **Frozen suites (pre/post SHA-256 verified):** Unseen `240`-row holdout (SHA-256 `ddf7649cd1559438...`) rose `147 / 240 -> 219 / 240` exact (`240 / 240` bit-identical to `cb14`'s frozen predictions); `probe1` rose `44 / 81 -> 72 / 81`.
- **29-function full witness bank (`192,972` rows):** **`172,505 -> 173,725` (`+1,220 / -0`, zero regressions)**: `BETA.INV` `5,571 -> 6,596 / 7,636` (`+1,025`), `BETAINV` `4,722 -> 4,754 / 4,963` (`+32`), `BETA.DIST` `3,965 -> 4,127 / 15,845` (`+162`), `BETADIST` `4,831 -> 4,832 / 5,231` (`+1`), all `25` other callers `+0 / -0`. The diff removed more lines than it added (`166` insertions, `270` deletions), with `0` pinned witness changes.

## Stage 2 (`dc-i2wk3`, commit `8ac7c9e8d`, and `dc-iuwct.51`, commit `b0c75eb2e`): the kept `Ext80` `bgrat`

When one shape parameter is large (`a = 500` up to `10^10`, routine in `T.DIST.2T`, `TDIST`, and `T.DIST.RT` at high degrees of freedom) and the other is small, `bratio` dispatches to the asymptotic gamma expansion `bgrat`. Its leading exponent is:

```text
g = (a + b - 0.5) * ln(1 + b / a) - b - b * ln(1 + (b - 1) / (2a)) + s(a + b) - s(a)
```

Notice the trap: `(a + b - 0.5) * ln(1 + b / a) = b + O(1 / a)`. Rounding that first product to 53-bit `binary64` *before* subtracting `b` destroys `11` to `27` bits of significand right away. Storing `r = z * dens`, `u = r * exp(g)`, and `j_0 = q / r` in `binary64` injects a second rounding error into `u * j_0 = q * exp(g)`.

Commit `8ac7c9e8d` (`dc-i2wk3`, lane `xl2`) fixed both defects in one portable generic `<F: Fpu>` x87 extended-precision routine (`excel_beta_bgrat`):

- It keeps `bgrat`'s intermediate recurrence (`eg`, `u`, `j`, `c`, `cn`, `t`, `summ`) in 80-bit extended precision (`Ext80`), evaluating the cancellation-prone log terms through Excel's `excel_rlog1` (`e - ln(1 + e)`).
- It **stores `u * summ` to `binary64` before adding the finite-recurrence seed `w`**.
- For fractional `b < 1, b != 0.5`, it unifies both `z >= 1.1` and `z < 1.1` onto Excel's shared `regularized_gamma_q(b, z)`, deleting `278` lines of dead NSWC `bratio_grat1` / `bratio_erf_nswc` / `bratio_erfc1` / `bratio_rexp` duplicates.

That landing resolved **`139 / 139`** `bgrat` discrepancies in the Windows `ww2` suite (`TDIST` `98 -> 0` [`382 / 382`], `T.DIST.RT` `14 -> 0` [`42 / 42`], `T.DIST.2T` `14 -> 0` [`42 / 42`], `BETA.DIST` `18 -> 5`) and gained **`+4,366 / -45` (`net +4,321`)** across the witness bank (`BETA.DIST` `+2,194`, `T.DIST.2T` `+1,299`, `TDIST` `+297`, `T.DIST.RT` `+274`, `T.DIST` `+152`), bringing the 29-caller bank to `178,046 / 192,972`.

Shortly afterwards, `dc-iuwct.51` (`b0c75eb2e`, lane `nx12`) fixed the damped-Newton `(factor * df) / density` step in `beta_newton_lower` and added x87-stored TwoSum exponent accumulation in `beta_density_at` for `a < 1 || b < 1`, eliminating the `258,048`-ULP gross `LowerEndpoint` miss `$[7]` and advancing the 29-caller bank from `178,046` to **`178,319 / 192,972`** (`+273 / -0`: `BETA.INV` `+243`, `BETA.DIST` `+21`, `BETAINV` `+9`).

### Why `dc-i2wk3`'s `Ext80` `bgrat` stayed when `cb18` arrived

Carbon `cb18` researched `bfrac` and `bgrat` against the pre-`dc-i2wk3` snapshot (`4b1be41f`) and independently derived its own `Ext80` `bgrat` where `lw + lu * summ` stayed unspilled in `Ext80` and `q` still kept separate `grat1` paths. When Neon `nx17` landed `cb18` on top of `dc-i2wk3`, it ran both `Ext80` `bgrat` routines head-to-head on `cb18`'s own `120` `bgrat` rows across `probe1` and `holdout`. **`dc-i2wk3`'s `excel_beta_bgrat` won `118 / 120` against `109 / 120` (`+9` exact, `0` losses)**—including **`24 / 24` vs `18 / 24`** on unseen `H5` (for instance `BETA.DIST(0x3fe953012ed428e2, 485, 1.5, TRUE) = 0x35ebd21ec4a15046`, where storing `u * summ` before `+ w` matches Excel's exact word) and **`7 / 8` vs `4 / 8`** on `P6C`—while keeping pinned `BETA.DIST` at `105 / 120` instead of `102 / 120`. Keeping `dc-i2wk3`'s `Ext80` `bgrat` and taking `cb18`'s `bfrac` beat either one alone.

## Stage 3 (`cb18` / `nx17`, commit `9b35d0fce`): Eq. (2.8) in Modified Lentz (`+4,576 / -81`)

With `bgrat` solved, the largest remaining cluster of misses sat inside `bfrac`, the continued-fraction arm for interior large shapes. Our code ran TOMS 708 `SUBROUTINE BFRAC`—a forward Wallis recurrence `(an, bn, anp1, bnp1)` re-parameterized with `(t0, p, c0, c1)` and stopped at `<= EPS * r`. Even on exact dyadic inputs where the density prefactor has almost no rounding noise, that recurrence drifted `2..8` ULP from Excel (`83 / 586` exact on earlier `bfrac` probes).

Carbon `cb18` proved that Excel does not run TOMS 708's Wallis recurrence at all. Instead, Excel evaluates **DiDonato & Morris (1992) §2 Eq. (2.8)** directly inside the exact same **Thompson–Barnett Modified-Lentz template** already proven for Excel's gamma continued fraction ([last-bit-gamma.md](last-bit-gamma.md)), storing each named intermediate to `binary64` at x87 `PC = 64` (`CW = 0x133F`):

```text
TINY     = 1.0e-30
STOP_TOL = 10.0 * f64::EPSILON          // 0x3cd4000000000000

ab  = st(a + b)
lam = st(a - st(ab * x))                // unconditional; never st(ab * y) - b
c   = st(1 + lam)
yp1 = st(y + 1)
ap1 = st(a + 1)
b0  = st(st(a / ap1) * c)

f = cl = TINY;  dl = (|b0| < TINY) ? TINY : b0
// step 0 Modified Lentz update on b0, then s = ap1:

for n = 1, 2, ...:
    sp2   = st(s + 2)
    f1    = st(st(a + (n - 1)) / s)
    f2    = st(st(ab + (n - 1)) / s)
    f3    = st(n * st(b - n))
    nox   = st(st(f1 * f2) * f3)
    if !nox.is_finite() { return NaN }  // reproduces 24/25 dc-iuwct.50 #NUM! edges
    alpha = st(st(nox * x) * x)         // left-to-right, not nox * st(x * x)
    w     = st((n * (b - n)) * x)       // x87 FST idiom: n*(b-n) stays 80-bit in ST(0)
    e2    = st(st(a + n) / sp2)
    ct    = st(c + st(n * yp1))
    beta  = st(st(n + st(w / s)) + st(e2 * ct))
    s     = sp2
    // Modified Lentz dl, cl (clamping |dl|, |cl| < TINY to TINY), del = st(cl * dl), f = st(f * del)
    if |del - 1.0| <= STOP_TOL { break }

return st(bf * f)
```

Notice the bonus tucked inside `nox`: computing `nox = st(st(f1 * f2) * f3)` *before* multiplying by `x` twice means that for huge shape parameters, `nox` overflows to `±Inf` even when the true mathematical `alpha = nox * x^2` would still be finite. Returning `NaN` when `!nox.is_finite()` reproduces **`24 / 25`** of the `dc-iuwct.50` `#NUM!` edges automatically, with no separate range guard.

### How single probes pinned down every store point and constant

Every line of that loop had plausible mathematical rivals that agree to within one or two ULPs on typical inputs. Carbon `cb18` synthesized single-purpose probes where two rivals differed by `1` ULP and froze its predictions with SHA-256 hashes before asking Excel:

- **`TINY = 1e-30` vs `1e-50` (`P2A`):** On `BETA.DIST(0.146683454955765, 55, 102, TRUE)` (`x = 0x3fc2c68600f31c42`), step `0` computes `st(b0 + st(1 / TINY))` in `binary64`. `TINY = 1e-30` predicts `0x3de34bd0fa7a0417`; `TINY = 1e-50` predicts `0x3de34bd0fa7a0418`. Excel returns `0x3de34bd0fa7a0417` (**`12 / 12` vs `0 / 12`**).
- **Factoring of `b0` (`P2B`):** On `BETA.DIST(0.1581686567276954, 129, 93.5, TRUE)` (`x = 0x3fc43ededbf2e7e7`), both `c / (1 + 1/a)` and `st(a * c) / (a + 1)` predict `0x365f4cfb10dc8b6a`, while `st(st(a / (a + 1)) * c)` predicts `0x365f4cfb10dc8b69`. Excel returns `0x365f4cfb10dc8b69` (**`12 / 12` vs `0 / 12`**).
- **Exact stopping boundary `|del - 1| <= 10 * EPS` (`P3A` / `P3B` / `P3C`):** `cb18` placed probes inside `(9 * EPS, 10 * EPS)`, at exact equality `|del - 1| == 10 * EPS`, and inside `(10 * EPS, 11 * EPS)`. On `P3B` (`BETA.DIST(0.42737455191265206, 99, 69, TRUE)`, `x = 0x3fdb5a1acae6e132`), `<= 10 * EPS` exits one iteration earlier (`0x3eea2121d7525c45`) while strict `< 10 * EPS` runs one more iteration (`0x3eea2121d7525c46`). Excel returns `0x3eea2121d7525c45`. Across the three brackets: **`12 / 12`** vs `0 / 12` (`<= 9 * EPS`), **`8 / 8`** vs `0 / 8` (`< 10 * EPS`), and **`12 / 12`** vs `0 / 12` (`<= 11 * EPS`).
- **Multiplication order and the x87 `FST` idiom (`P4A`–`P4D`):**
  - Left-to-right `alpha = st(st(nox * x) * x)` beat `nox * st(x * x)` **`9 / 9` vs `0 / 9`** (e.g. `BETA.DIST(0.22182208965581077, 115, 243, TRUE)` -> `0x3ee0ecafff4aa512` vs `0x3ee0ecafff4aa514`).
  - `e2 = st((a + n) / (s + 2))` and `ct = st(c + n * yp1)` beat TOMS `e2` and `1 + (lam + n * yp1)` **`8 / 9` vs `0 / 9`**.
  - Keeping `n * (b - n)` unspilled in x87 `ST(0)` for `w = st((n * (b - n)) * x)` (the same `FST` idiom seen in `atanh_tail`) beat reloading stored `f3` (`st(f3 * x)`) **`8 / 10` vs `1 / 10`** (e.g. `BETA.DIST(0.1875, 76, 151.00000000036408, TRUE)` -> `0x3e7534b85b705210` vs `0x3e7534b85b705211`).
  - Unconditional `lam = st(a - st(ab * x))` beat `st(ab * y) - b` **`7 / 8` vs `1 / 8`** (`+1,056` net bank rows).

**Stage 3 evidence (`cb18` / `nx17`, `9b35d0fce`, checked):**
- **Frozen suites (filtered against `337,842` historical keys, pre/post SHA-256 verified):**
  - `Probe 1` (`200` rows, `15` functions, SHA-256 `53bcb2f79ae6e108...`): `25 / 200` (`cb18` baseline `4b1be41f`) / `75 / 200` (pre-`9b35d0fce` HEAD) -> **`178 / 200` exact** (`<= 1u: 185`, `<= 2u: 193`), beating `cb18`'s `175 / 200` by `+3`.
  - Unseen `holdout` (`240` rows, `17` functions, SHA-256 `73284af7c201570d...`): `23 / 240` (`cb18` baseline) / `89 / 240` (HEAD before) -> **`187 / 240` exact** (`<= 1u: 208`, `<= 2u: 231`), beating `cb18`'s `181 / 240` by `+6` (`H4` `T.DIST*` `48 / 48`, `H5` `bgrat` `24 / 24`, `H1` `38 / 48`, `H6` `INV` `31 / 40`).
- **29-function full witness bank (`192,972` rows):** **`178,319 -> 182,814`** (**`+4,576 / -81`, net `+4,495`, `56.5 : 1`**, `0` functions regressed, `0` pinned witness changes).
- **Independent Windows confirmation (`ww7` on `9b35d0fce`):** Reproduced `cb18` holdout `187 / 240` and `probe1` `178 / 200`, `soft-x87` unit tests `561` passed (`3` ignored), and pinned witness tests passing. In the `ww2` discrepancy suite, `NEGBINOM.DIST` `f = 39` `bfrac` dropped `13 -> 0`, and `dc-dnbcq` `ours-wrong` rows dropped `942 -> 659` (`BETA.DIST`) and `35 -> 17` (`BETADIST`).

## Cumulative movement across the 29 incomplete-beta callers

Every step below was measured on the same `192,972`-row, 29-function full witness bank (Linux `dna-firehorse`, zero functions regressed at any step):

| Stage & Commit | Exact (`192,972` rows) | Net Delta | Primary Movers |
|:---|---:|---:|:---|
| Before `cb14` (`dc-j9k.84`) | 172,505 | — | `20,467` misses across 29 callers |
| `cb14` / `nx12` (`a423c5c0d`) | 173,725 | +1,220 (`+1,220 / -0`) | `BETA.INV` `+1,025`, `BETA.DIST` `+162`, `BETAINV` `+32`, `BETADIST` `+1` |
| `dc-i2wk3` / `xl2` (`8ac7c9e8d`) | 178,046 | +4,321 (`+4,366 / -45`) | `BETA.DIST` `+2,194`, `T.DIST.2T` `+1,299`, `TDIST` `+297`, `T.DIST.RT` `+274`, `T.DIST` `+152` |
| `dc-iuwct.51` / `nx12` (`b0c75eb2e`) | 178,319 | +273 (`+273 / -0`) | `BETA.INV` `+243`, `BETA.DIST` `+21`, `BETAINV` `+9` |
| `cb18` / `nx17` (`9b35d0fce`) | **182,814** | **+4,495 (`+4,576 / -81`)** | `BETA.DIST` `+2,250`, `BINOM.DIST` `+680`, `BETA.INV` `+362`, `NEGBINOM.DIST` `+361`, `BINOMDIST` `+212`, `F.DIST` `+170`, `BETADIST` `+133`, `F.INV` `+69`, `FINV` `+63`, `BETAINV` `+62`, `F.INV.RT` `+57`, `F.DIST.RT` `+53`, `FDIST` `+23` |
| **Total (November 22)** | **182,814 / 192,972** | **+10,309** | **Misses cut `20,467 -> 10,158` (`94.74%` exact)** |

Selected function totals after `9b35d0fce`: `BETA.DIST` `8,592 / 15,845`, `BETADIST` `4,997 / 5,231`, `BETA.INV` `7,220 / 7,636`, `BETAINV` `4,828 / 4,963`, `BINOM.DIST` `71,044 / 71,362`, `BINOMDIST` `3,136 / 3,277`, `NEGBINOM.DIST` `3,807 / 3,953`, `F.DIST` `3,911 / 4,750`, `F.DIST.RT` `2,979 / 3,071`, `FDIST` `4,207 / 4,520`, `F.INV` `4,388 / 4,425`, `F.INV.RT` `4,378 / 4,434`, `FINV` `4,411 / 4,463`, and `T.DIST.2T` `5,470 / 5,487` (`99.69%`).

## What is checked and what stays open

**Checked:** Every code change above—deleting the five `beta_inverse` bounding boxes and tiny-shape patch (`a423c5c0d`), keeping `dc-i2wk3`'s `Ext80` `bgrat` with stored `u * summ` before `+ w` (`8ac7c9e8d`), fixing the `LowerEndpoint` damped-Newton step (`b0c75eb2e`), and replacing Wallis `bfrac` with DiDonato & Morris Eq. (2.8) in Modified Lentz (`9b35d0fce`)—is verified on live Excel `16.0.20527.20196` frozen holdouts, on the full `192,972`-row witness bank, under both native x87 (`x86_64`) and software x87 (`soft-x87` / `wasm32`), and independently on Win64 (`ww7`).

**Open (`10,158` misses remaining across the 29 callers):**

- **`dc-j9k.55` (the density prefactor, `ln B` for `a, b >= 8`, and longer `bup` recurrences):** Now that the `bfrac` and `bgrat` inner loops match Excel, they no longer hide upstream offsets. Almost all of the remaining `1..5` ULP misses on non-dyadic `bfrac` and `bup -> bgrat` rows—and the `-81` bank boundary flips—trace to **`beta_density_at` / `binom_exponent` (the density prefactor)** and to **`excel_ln_beta` for `a, b >= 8`** (where a single ULP in `ln B(a, b)` magnifies to `11..58+` ULP inside `bpser`, driving `982` `17..100`-ULP `BETA.DIST` misses and the remaining `659` `BETA.DIST` + `17` `BETADIST` `dc-dnbcq` `ours-wrong` rows). Also open under `dc-j9k.55`: store ordering in `bup` for `b > 1, n >= 4` (`±1..2` ULP) and `b <= 1` twenty-step (`±1..3` ULP), the `11` `nx4` classified pinned rows, and Windows workspace-dependents, browser-sweep, and `perf`-profile parity checks.
- **Sibling beads owning specific caller paths:** `GAMMA.DIST` `a < 1` `st190` (`dc-j9k.5`), the `f_tails` `(1 / (1 + r), r / (1 + r))` pair (`dc-j9k.44`), `bup_160` near-`DBL_MIN` (`dc-j9k.86`), remaining `BETA.INV` / `BETAINV` residuals including the sole remaining gross row `$[6]` (`121,408` ULP, amplified across `81` Newton steps from a `1`-ULP `bup` tie at iteration `1`) (`dc-j9k.70`), `F.DIST` PDF (`dc-j9k.73`), F quantiles and `F.TEST` (`dc-j9k.74`), T-family residuals (`dc-j9k.75`), and `BINOM.DIST` PMF (`dc-j9k.76`).
