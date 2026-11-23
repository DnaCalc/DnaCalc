# The last bit: Excel's PMT and amortization family

*Researched in Carbon round `cb19` and landed in Neon round `nx18` (2026-11-23), commit `b4bd656db`, closing beads `dc-j9k.90` and `dc-j9k.12`.*

Five financial worksheet functions share Excel's loan-amortization arithmetic: `PMT` (periodic payment), `IPMT` (interest in one period), `PPMT` (principal in one period), `CUMIPMT` (cumulative interest across a range of periods), and `CUMPRINC` (cumulative principal across a range of periods). Across all `308,710` recorded Excel answers in our full witness bank—`291,978` of them for `PMT` alone—all five functions now match Excel down to the last bit of every `binary64` double (`308,710 / 308,710`, `100.000%`), along with all `409 / 409` original `dc-j9k.12` oracle-corpus cells and `240 / 240` unseen frozen-holdout inputs.

As with `NORMSINV` ([last-bit-normsinv.md](last-bit-normsinv.md)) and the gamma family ([last-bit-gamma.md](last-bit-gamma.md)), everything below was inferred strictly from recorded and live Excel answers (build `16.0.20527.20196` via Touchstone) and public IEEE 754 / x87 arithmetic and annuity algebra. No Microsoft source code was read, disassembled, or decompiled.

## Where we started

Before commit `b4bd656db`, our five-function amortization bank already stood at `308,703` of `308,710`—only seven misses remained in the full bank, plus one stubborn ignored test:

- **`PMT` (`1` miss in `291,978`):** `round3-pmt-balance-0529` missed by `1` unit in the last place (ULP): `0xff88618618618618` instead of Excel's `0xff88618618618617`.
- **`IPMT` (`3` misses in `4,411`, worst `24` ULP):** `IPMT[819]` (`-4` ULP), `IPMT[1670]` (`-1` ULP), and `IPMT[2389]` (`-24` ULP).
- **`PPMT` (`3` misses in `4,387`, worst `2` ULP):** `PPMT[200]` (`+1` ULP), `PPMT[1628]` (`+2` ULP), and `PPMT[2154]` (`-1` ULP).
- **`CUMIPMT` (`4,393 / 4,393`) and `CUMPRINC` (`3,541 / 3,541`):** already exact on the bank, yet hiding subtle rounding differences on double-rounding multiplication ties and fractional period counts.
- **One ignored `246`-ULP mystery (`first_adjacent_period_open_exception` in `f3_financial.rs`):** `IPMT(0.24042490731562907, 1 + 2^-52, 1 - 2^-53, -486.83764968944774, 111969.28369137854)` missed Excel (`0x405d4310bdc21078`) by `246` ULP, even though the `(nper + 1) - per` remaining-period grouping matched `2,997` of `3,000` fresh controls.

Those seven bank misses and one `246`-ULP puzzle turned out not to be seven quirks. They were the footprints of **three places where our code had invented extra complexity or kept plain `binary64` arithmetic instead of Excel's simpler, shared x87 `PC = 64` program**, plus one grouping rule in `CUMPRINC` that we confirmed with direct probes.

## Insight 1: One unified balance formula (no `fv == -pv` or negative-rate splits)

Every intermediate arithmetic step in Excel's legacy financial path runs in x87 64-bit significand precision (`PC = 64`, round-to-nearest-even, `CW = 0x133F`) and rounds back to IEEE `binary64` when stored—written `st(...)` below.

**What was wrong before:** Our `pmt()` function split nonzero rates across two legacy special branches:

1. An offsetting-balance branch (`rate > 0 && pv != 0 && fv == -pv`) that computed beginning-of-period (`type = 1`) payments by dividing `fv / st(1 + st(1 / r))` instead of multiplying by the stored due coefficient `due(r) = st(1 / st(1 + st(1 / r)))`.
2. A sign split (`if neg_log < 0 || rate < 0 { ... } else { ... }`) that sent positive rates with negative periods (`rate > 0, nper < 0`, so `neg_log > 0`) down an unported plain `binary64` path computing `pv + fv * (1 + em)` and `(q / tf) * r`. That produced `1..64` ULP drift, wrong `type = 1` rounding, and spurious `#NUM!` overflows whenever `fv * (1 + em)` exceeded `f64::MAX`.

**What Excel does:** One six-line formula covers every nonzero rate (positive or negative) and every period sign (positive or negative), once `1 + r > 0` and the `type = 1` pole `st(1 + st(1 / r)) == 0` are checked:

```text
neg_log  = -st(nper * L)                        // L = alnrel(r) = ln(1 + r)
em       = expm1_spilled(neg_log)               // em == 0 or !em.is_finite() -> #NUM!
quotient = st(st(st(pv + fv) / em) + fv)
payment  = st(quotient * due(r))                // type = 1, due(r) = st(1 / st(1 + st(1 / r)))
payment  = st(quotient * r)                     // type = 0
```

Deleting both branches handles every corner automatically. When `fv == -pv`, `pv + fv` is exact `0.0`, so `0.0 / em` is `0.0` and `quotient` reduces cleanly to `fv`. Multiplying `fv * due(r)` avoids overflowing `fv * r` on huge balances when `type = 1` (while naturally overflowing to `#NUM!` when `type = 0` and `|fv * r| > f64::MAX`). And when `nper < 0`, dividing `(pv + fv) / em` never forms the overflowing product `fv * (1 + em)`.

**How a single probe told the rivals apart:**
- On `PMT`'s lone full-bank failure `round3-pmt-balance-0529`—`PMT(0.05, 1, 4.4942328371557893e307, -4.4942328371557893e307, 1)` (`rate = 0x3fa999999999999a`, `pv = 0x7fcfffffffffffff`, `fv = -pv`)—the old branch divided `fv / st(1 + 1 / r)` and got `0xff88618618618618`. Multiplying `st(fv * due(r))` gets `0xff88618618618617`, matching Excel's exact word.
- On `cb19-probe1-0037` (`rate = 0x3fdb06e8efcc9a3d`, `nper = 0xc05a26dd70f17112`, `pv = 0xff01b438bf6d40c6`, `fv = 0xff62d9384bc533a2`, `type = 1`), the old `nper < 0` branch overflowed `fv * (1 + em)` to `#NUM!`, while the unified formula returned `0xff4662a8e3bad70c`, matching Excel bit-for-bit.
- Across all live probes (`P1a`, `P1b`, `P2a`, `P2b`, `P2c`, `H2a`+`H2b`), the single unified body beat the split code **`12 / 12` vs `0 / 12`** on normal `fv == -pv`, **`8 / 8` vs `0 / 8`** when `|fv * r| > f64::MAX`, **`8 / 8` vs `0 / 8`** on `r > 0, nper < 0, fv != 0`, **`8 / 8` vs `0 / 8`** on `r > 0, nper < 0, type = 1`, **`8 / 8` vs `0 / 8`** where the old code overflowed to `#NUM!`, and **`24 / 24` vs `0 / 24`** on unseen holdouts.

## Insight 2: The shared `PC = 64` logarithm instead of a separate `atanh` series

**What was wrong before:** To compute `ln(1 + r)` without cancellation near `r = 0`, our `alnrel(rate)` helper called a separate 12-term `binary64` backward Horner polynomial (`excel_log1p_atanh`), summing `s = r / (2 + r)` up to `s^23 / 23`. For negative rates near `-0.35`, `|s|` reaches `0.225`, where truncating at `s^23 / 23` leaves a tiny error that `IPMT` and `PPMT` magnify dramatically whenever `pv` and `fv` nearly cancel.

**What Excel does:** Excel never maintained a separate `binary64` `atanh` polynomial for finance. It calls the **exact same x87 `PC = 64` forward `atanh` logarithm** (`excel_log1p_times(1.0, rate)`) already shared by `BINOM.DIST`, `GAMMA.DIST`, and `BETA.DIST`:

- For `|r| < 0.375` (`3 / 8`): store `q = st(2 + r)`, form `v = r / q`, sum the odd-power tail `v^3 / 3 + v^5 / 5 + ...` forward until the stored `binary64` tail stops changing (using the x87 `FST` idiom where `power *= w` stores to `binary64` while keeping the 80-bit product in `ST(0)` for the division by `2j + 1`), then store `2 * st(st(v) + tail)`.
- For `|r| >= 0.375`: call the x87 `FYL2X` logarithm `excel_log(st(1 + r))`.

**How a single probe settled it—and solved the `246`-ULP mystery:**
- Straddling `±0.375` (`P5`, `16` probes), switching at `0.375` matched **`16 / 16`** against **`6 / 16`** for calling `ln(1 + r)` directly.
- Switching `alnrel` from `excel_log1p_atanh` to `excel_log1p_times(1.0, rate)` shifted `alnrel(0.24042490731562907)` by a single ULP (`0x3fcb93ff0c5cd0b9 -> 0x3fcb93ff0c5cd0ba`). That single bit instantly dropped the ignored test `first_adjacent_period_open_exception` from **`246` ULP to `0` ULP** (`0x405d4310bdc21078`) under the standard `(nper + 1) - per` order—proving there had never been a remaining-period grouping exception at all.
- It simultaneously cleared **all `3 / 3` `IPMT` bank misses** (`IPMT[819]` `-4 -> 0` ULP, `IPMT[1670]` `-1 -> 0` ULP, `IPMT[2389]` `-24 -> 0` ULP where `|v| = 0.225` had been amplified `45x` by `fv` cancellation) and **all `3 / 3` `PPMT` bank misses** (`PPMT[200]` `+1 -> 0` ULP, `PPMT[1628]` `+2 -> 0` ULP, `PPMT[2154]` `-1 -> 0` ULP), and matched `5,999 / 6,000` (`99.98%`: `IPMT` `3,000 / 3,000`, `PPMT` `2,999 / 3,000`) on `fx2`'s held-out study. The bespoke `excel_log1p_atanh` helper was deleted outright.

## Insight 3: `PC = 64` stored exponent products (`44 / 44` vs `0 / 44` on double-rounding ties)

Why can multiplying two IEEE `binary64` numbers in x87 `PC = 64` and storing to `binary64` disagree with plain `binary64` multiplication?

The exact product of two 53-bit significands has up to `106` bits. A plain SSE2 `mulsd` rounds that exact product straight to `53` bits. An x87 `fmul` at `PC = 64` followed by a store (`fstp qword`) rounds it **twice**: first to `64` bits in the x87 register, then to `53` bits on store. Almost all the time both paths agree. Roughly once in `2^12` (`~1 / 4,096`) products, however, the exact product sits just barely short of a 53-bit halfway midpoint—close enough that rounding to `64` bits pulls it *onto* the exact halfway tie, and the second round-to-nearest-even flips it across to the opposite `binary64` neighbor.

**What was wrong before:** `period_components` already computed `-excel_x87_mul(remaining, L)`, whereas `pmt`, `end_payment_quotient`, and `cumulative_principal` still wrote plain `binary64` `-(periods * L)`, `-(last * L)`, and `-(count * L)`.

**How a single probe separated `PC = 64` from `binary64`:**
- Pick an input whose product `nper * L` falls on a double-rounding tie: `cb19-probe1-0173` (`PMT(0x3fc33b6afaaddbc3, 0x402206b00db5a8c8, 0x40f1b6afd7ffb1e3, 0, 1)`, i.e. `r ≈ 0.15025`, `nper ≈ 9.013`, `pv ≈ 72,555`, `type = 1`). Plain `binary64` `-(nper * L)` produces `0xc0c9d2d2c216746a`; x87 `PC = 64` `-excel_x87_mul(nper, L)` produces `0xc0c9d2d2c2167469`—one bit lower. Excel returns `0xc0c9d2d2c2167469`.
- To isolate `last * L` and `count * L` inside `CUMPRINC` and `CUMIPMT` without `nper * L` interfering, `cb19` set `nper = 65536` (`2^16`, an exact power of two that shifts only the exponent and can never double-round) and placed double-rounding ties strictly on `last * L` and `count * L` (`H5b`, `H6b`).
- Across all `44` live double-rounding multiplication ties (`P9` `16 / 16`, `H2c` `12 / 12`, `H5b` `8 / 8`, `H6b` `8 / 8`), `PC = 64` `-excel_x87_mul` matched **`44 / 44` (`100%`)** against **`0 / 44` (`0%`)** for plain `binary64` multiplication.

## Insight 4: How `CUMPRINC` groups `last`

Across a whole-period range `first..=end_period`, `CUMPRINC` (and `CUMIPMT`, which shares it) computes cumulative principal from `PMT`'s stored quotient `q = pv / expm1(-nper * L)` via:

```text
last     = (nper - end_period) + 1.0
discount = exp(-st(last * L))
count    = end_period - first + 1
span     = -expm1_spilled(-st(count * L))
total    = st(q * st(discount * span))          // multiplied by st(1 + r) for type = 0
```

For integer `nper`, `(nper - end_period) + 1.0` and `(nper + 1.0) - end_period` produce the exact same double. But Excel accepts fractional `nper` in `CUMPRINC` and `CUMIPMT`, and for fractional `nper` the parentheses change the last bit.

Take `cb19-probe1-0189`: `CUMPRINC(0x3fc054b1a7868af9, 0x3ff100000000009d, 0x40e0e6fbde64924f, 1, 1, 0)`, where `nper = 1.0625 + 157 * 2^-52` and `end_period = 1`. Adding `1.0` first pushes `nper + 1` into `[2, 4)`, rounding off the bottom bit of `nper` before subtracting `1`, which predicts `0xc0dfb1c6cfb3c551`. Subtracting `end_period` first drops `nper - 1` into `[1/16, 1/8)`, preserving that bit before adding `1.0`, which predicts `0xc0dfb1c6cfb3c550`.

Excel returned `0xc0dfb1c6cfb3c550`. Across all `12` fractional-`nper` `CUMPRINC` and `CUMIPMT` probes (`P10`), Excel matched **`(nper - end_period) + 1.0` on `12 / 12`** against **`0 / 12`** for `(nper + 1.0) - end_period`—the opposite grouping from `IPMT` / `PPMT`'s `(nper + 1) - per`, and confirmed to remain as written in `cumulative_principal`.

## Before and after numbers (`b4bd656db`)

### Pre-oracle frozen suites (`441` live Excel `16.0.20527.20196` calls)

Every candidate input was filtered against all `316,124` historical `(function, args)` keys, frozen to disk, and SHA-256 hashed **before** asking Touchstone (pre-oracle and post-oracle hashes verified identical):

| Suite | SHA-256 | Asked | Before (`origin/master`) | After (`b4bd656db`) | Notes |
|:---|:---|---:|---:|---:|:---|
| Health check (`SQRT(4)`) | — | 1 | 1 / 1 | **1 / 1** | — |
| `Probe 1` (`5` fns, `16` cats) | `8159363124a3fbb9...` | 200 | 47 / 200 (`23.5%`) | **181 / 200 (`90.5%`)** | `P1` `20/20`, `P2` `24/24`, `P5` `16/16`, `P9` `16/16`, `P10` `12/12`; `19` misses in tie-filtered `atanh_tail` (`dc-j9k.91`) |
| **Unseen `Holdout`** (`5` fns, `11` cats) | `5e2b6b35ad98075b...` | **240** | 187 / 240 (`77.9%`) | **240 / 240 (`100.0%`)** | `H1` `48/48`, `H2` `36/36`, `H3` `48/48`, `H4` `48/48`, `H5` `30/30`, `H6` `30/30` |
| **Total** | — | **441** | 235 / 441 (`53.3%`) | **422 / 441 (`95.7%`)** | **`206 / 206` (`100%`)** on all `206` live rows where `origin/master` differed from Excel |

### Full witness bank (`308,710` target rows; `328,915` across all 10 time-value functions)

| Function | Rows | Before (`origin/master`) | After (`b4bd656db`) | Delta (`+W / -L`) | Worst ULP | Class (`Before -> After`) |
|:---|---:|---:|---:|:---:|:---:|:---|
| `PMT` | 291,978 | 291,977 | **291,978** | `+1 / -0` | `1 -> -` | `last_bit -> exact` |
| `IPMT` | 4,411 | 4,408 | **4,411** | `+3 / -0` | `24 -> -` | `numeric -> exact` |
| `PPMT` | 4,387 | 4,384 | **4,387** | `+3 / -0` | `2 -> -` | `last_bit -> exact` |
| `CUMIPMT` | 4,393 | 4,393 | **4,393** | `+0 / -0` | `-` | `exact` |
| `CUMPRINC` | 3,541 | 3,541 | **3,541** | `+0 / -0` | `-` | `exact` |
| **Target 5 functions** | **308,710** | **308,703** | **308,710** | **`+7 / -0`** | — | **`100.000%` exact** |
| Siblings (`FV`, `PV`, `NPER`, `ISPMT`, `RATE`) | 20,205 | 20,193 | **20,193** | `+0 / -0` | unchanged | `0` outputs changed (`FV` `6,924/6,924`, `PV` `1,564/1,564`, `NPER` `2,509/2,509`, `ISPMT` `4,341/4,341`, `RATE` `4,855/4,867`) |

- **Oracle workbook corpus (`cargo xtask corpus --oracle`):** `156 / 156` workbooks, `78,344 / 78,390` formula cells (`99.94%`, `46` failed); all **`409 / 409`** original `dc-j9k.12` `catalogue` cells (`dcj9k_annuity` `404`, `prefix_metadata` `4`, `f3_financial` `1`) match, with **`0`** `PMT`/`IPMT`/`PPMT`/`CUMIPMT`/`CUMPRINC` mismatches across all `156` oracle workbooks.
- **Portability (`x86_64` hardware x87 vs `wasm32` `soft87`):** Bit-identical on all `308,710 / 308,710` bank rows and `440 / 440` live probe/holdout rows; `cargo test -p calc-func --features soft-x87 --lib` passed `556 / 556` (`3` ignored) and `cargo xtask check` passed `1,038 / 1,038` (`wasm32` ok), with `0` pinned witness changes.

## What is checked and what stays open

**Checked:** The unified `PC = 64` `PMT` formula, the shared `excel_log1p_times(1.0, rate)` logarithm, `PC = 64` stored exponent products (`44 / 44` on double-rounding ties), and `CUMPRINC`'s `(nper - end) + 1` grouping (`12 / 12`) are all checked on live Excel `16.0.20527.20196`, the `308,710`-row witness bank, the `156`-workbook oracle corpus, and native/software x87 parity.

**Open:**

- **`dc-j9k.91` (`x87_chains::atanh_tail` odd halfway-tie edge):** When `v_st + tail` inside shared `excel_log1p_times(1.0, rate)` lands within `0..32` `Ext80` units above an odd `binary64` halfway tie (`low11` in `0x400..0x420`, about `0.4%` of random rates), `atanh_tail` rounds `|L|` `1` ULP higher than Excel. That accounts for all `19 / 200` misses in `cb19`'s tie-concentrated `Probe 1` filter and `1 / 6,000` in `fx2` (`fx2[510]`), with `0 / 240` misses on the unseen holdout and `0 / 308,710` on the full bank. Finding the exact micro-arrangement without regressing `BINOM.DIST`, `GAMMA.DIST`, `BETA.DIST`, or amortization banks is tracked under `dc-j9k.91`.
- **Windows verification (tracked under parent `dc-j9k`):** Native Windows `cargo xtask check --dependents`, `--features soft-x87`, browser `sweep`, and `perf`-profile parity checks listed in `nx18`'s Windows handoff.
