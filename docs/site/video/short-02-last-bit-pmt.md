# Short 2: The last-bit story — cracking Excel's `PMT` to zero ULPs (`dnacalc.com`)

- **Status:** Draft (`dc-6q2k.14`)
- **Target length:** 2–3 minutes (~2:40)
- **Audience:** Engineers, numerical programmers, and spreadsheet auditors curious how DNA Calc finds exact `binary64` parity with Excel without reading Microsoft's source code.
- **The one thing shown:** How seven stubborn misses across `PMT`, `IPMT`, `PPMT`, `CUMIPMT`, and `CUMPRINC`—plus one `246`-ULP ignored test—collapsed to `308,710 / 308,710` exact matches (`100.000%`) using targeted Touchstone probes, double-rounding tie inputs, and frozen SHA-256 holdouts.
- **Paired article & blog post:** [`docs/articles/last-bit-pmt.md`](../../articles/last-bit-pmt.md) and [`docs/site/blog/2026-11-23-last-bit-pmt.md`](../blog/2026-11-23-last-bit-pmt.md).

## Exact files and setup

- **Bit explorer:** `docs/fun/dna-calc-brochure.html` (section `A4 · Try it`: sign + 11 exponent + 52 mantissa bits).
- **Article & tables:** `docs/articles/last-bit-pmt.md` (`cb19` / `nx18`, commit `b4bd656db`, `2026-11-23`).
- **Live witness table:** `crates/calc-func/witness-status.tsv` (`PMT`, `IPMT`, `PPMT`, `CUMIPMT`, `CUMPRINC`).

---

## Shot list and narration

### Shot 1 — Almost exact isn't exact: seven misses and one `246`-ULP mystery (0:00 – 0:30)

**On-screen actions**
1. Open `docs/articles/last-bit-pmt.md` on the "Where we started" table: `308,703` of `308,710` matched across `PMT`, `IPMT`, `PPMT`, `CUMIPMT`, and `CUMPRINC`.
2. Highlight the `7` remaining bank misses (`1` `PMT` in `291,978` at `1` ULP; `3` `IPMT` up to `24` ULP; `3` `PPMT` up to `2` ULP) and the ignored `246`-ULP `IPMT` test (`first_adjacent_period_open_exception`).

**Narration**
> "Before commit `b4bd656db`, our five loan-amortization functions—`PMT`, `IPMT`, `PPMT`, `CUMIPMT`, and `CUMPRINC`—already matched Excel on `308,703` out of `308,710` recorded answers.
>
> Only seven misses remained in the entire bank, plus one stubborn ignored `IPMT` test off by `246` ULPs. It looked like seven tiny edge quirks. In fact, it was three places where our code was more complicated than Excel's actual program."

---

### Shot 2 — Visual: why x87 double-rounding flips one multiply in `4,096` (0:30 – 1:10)

**On-screen actions**
1. Show `docs/fun/dna-calc-brochure.html` section `A4 · Try it` (`1` sign bit + `11` exponent bits + `52` mantissa bits = `binary64`) to illustrate the `53`-bit significand boundary.
2. Overlay / highlight Insight 3 from `docs/articles/last-bit-pmt.md`:
   - `106`-bit exact product → `53` bits (plain SSE2 `binary64` `mulsd`) vs `106` → `64` (x87 `PC = 64`) → `53` (store to `binary64`).
   - Show probe `cb19-probe1-0173`: plain `binary64` `-(nper * L)` gives `0xc0c9d2d2c216746a`; x87 `PC = 64` gives `0xc0c9d2d2c2167469` — matching Excel.
   - Show the `nper = 65536` (`2^16`) trick isolating `last * L` and `count * L`: **`44 / 44`** vs **`0 / 44`**.

**Narration**
> "First: double rounding. Multiplying two `53`-bit significands produces a `106`-bit exact product. Modern `binary64` hardware rounds that straight to `53` bits. Excel's legacy path multiplies in x87 `64`-bit significand precision and stores back to `binary64`, rounding twice: `106` to `64` to `53`.
>
> Roughly once in `4,096` multiplies, an exact product sits just shy of a halfway tie: the intermediate `64`-bit round pulls it onto the tie, and the store flips it to the neighbor.
>
> So we built double-rounding tie probes—even setting `nper = 65536`, an exact power of two that shifts only the exponent, to isolate `last * L` and `count * L` inside `CUMPRINC` and `CUMIPMT`. Across `44` live tie probes on Excel 16.0, x87 `PC = 64` matched `44 of 44` against `0 of 44` for plain `binary64`."

---

### Shot 3 — Two more probes: deleting special branches and a single-ULP `ln(1+r)` shift (1:10 – 1:55)

**On-screen actions**
1. Highlight Insight 1 (`docs/articles/last-bit-pmt.md:30-45`): show the old `fv == -pv` and `neg_log >= 0` special branches crossed out, leaving the six-line unified `PMT` formula and `round3-pmt-balance-0529` flipping from `0xff88618618618618` to Excel's `0xff88618618618617` (`12/12` and `8/8` on live probes).
2. Highlight Insight 2 (`docs/articles/last-bit-pmt.md:56-59`): show the probe straddling `±0.375` (`16 / 16` vs `6 / 16`) that pinned Excel's shared `excel_log1p_times(1.0, rate)` (`|r| < 0.375` x87 `FST` `atanh` loop, `FYL2X` above).
3. Show the single-bit shift `alnrel(0.24042490731562907)`: `0x3fcb93ff0c5cd0b9 -> 0x3fcb93ff0c5cd0ba`, dropping the `246`-ULP `IPMT` test straight to `0` ULP (`0x405d4310bdc21078`) and clearing all `3/3` `IPMT` and `3/3` `PPMT` misses.

**Narration**
> "Second: our `PMT` had two legacy special branches—one for `fv == -pv` and one for negative `nper`—that Excel doesn't have. Deleting both left one six-line `PC = 64` formula that fixed `PMT`'s lone bank miss on the spot (`...8618` to `...8617`) and eliminated spurious `#NUM!` overflows.
>
> Third: `alnrel`, `ln(1 + r)`. A probe straddling `±0.375` matched `16 of 16` when switching at `0.375` to the same shared `PC = 64` logarithm used by `BINOM.DIST` and `GAMMA.DIST`. Replacing our bespoke polynomial shifted `alnrel` by a single ULP—`...b9` to `...ba`—and that one bit instantly dropped our `246`-ULP `IPMT` puzzle to zero ULPs, clearing every remaining `IPMT` and `PPMT` bank miss."

---

### Shot 4 — Frozen SHA-256 verification and `308,710 / 308,710` (1:55 – 2:40)

**On-screen actions**
1. Scroll to the "Before and after numbers (`b4bd656db`)" tables in `docs/articles/last-bit-pmt.md`:
   - Highlight the pre-oracle SHA-256 hash (`5e2b6b35ad98075b...`) on the unseen `Holdout`: `187 / 240` → **`240 / 240` (`100.0%`)**.
   - Highlight the full witness-bank table (`PMT` `291,978`, `IPMT` `4,411`, `PPMT` `4,387`, `CUMIPMT` `4,393`, `CUMPRINC` `3,541` → **`308,710 / 308,710`**, `100.000%`), plus `0` regressions on `20,205` sibling rows (`FV`, `PV`, `NPER`, `ISPMT`, `RATE`).
2. Briefly show `crates/calc-func/witness-status.tsv` confirming `exact` across all five functions, and point to the `dc-j9k.91` open note at the bottom of `last-bit-pmt.md`.

**Narration**
> "Before we asked Excel a single verification question, we froze `240` unseen inputs to disk and SHA-256 hashed the prediction file. After landing the fix, Touchstone Excel `16.0` matched our predictions on `240 of 240`—up from `187`.
>
> Across the full witness bank, all five amortization functions now hit `308,710 of 308,710` bit-for-bit (`100.000%`), running identically on hardware x87 and our portable software x87 in WebAssembly. Only one tiny `~0.4%` odd halfway-tie edge in the shared `atanh` tail stays open under `dc-j9k.91`—tracked like every other remainder until we nail its exact micro-step."

---

## Facts relied on and sources

| Fact used in script | Exact source in repository |
|---|---|
| Pre-fix baseline `308,703 / 308,710` (`PMT` `1` miss in `291,978`; `IPMT` `3` misses in `4,411` worst `24` ULP; `PPMT` `3` misses in `4,387` worst `2` ULP; `CUMIPMT` `4,393/4,393`; `CUMPRINC` `3,541/3,541`) and ignored `246`-ULP `IPMT` test (`first_adjacent_period_open_exception`) | `docs/articles/last-bit-pmt.md:11-17`, `crates/calc-func/witness-status.tsv` (`PMT`, `IPMT`, `PPMT`, `CUMIPMT`, `CUMPRINC`) |
| `106`-bit exact product double-rounding (`106 -> 64 -> 53`, roughly `1 / 4,096` products), `cb19-probe1-0173` (`0xc0c9d2d2c216746a` vs `0xc0c9d2d2c2167469`), `nper = 65536` (`2^16`) isolation, and `44 / 44` vs `0 / 44` probe win | `docs/articles/last-bit-pmt.md:61-72`, `docs/site/blog/2026-11-23-last-bit-pmt.md:5-7, 15` |
| Unified 6-line `PMT` body clearing `round3-pmt-balance-0529` (`0xff88618618618618 -> 0xff88618618618617`) and `cb19-probe1-0037` `#NUM!` overflow (`12/12`, `8/8`, `8/8`, `8/8`, `8/8` on live probes) | `docs/articles/last-bit-pmt.md:21-45` |
| Shared `excel_log1p_times(1.0, rate)` (`±0.375` switch `16 / 16` vs `6 / 16`), `alnrel(0.24042490731562907)` shifting `1` ULP (`0x3fcb93ff0c5cd0b9 -> 0x3fcb93ff0c5cd0ba`) and dropping `first_adjacent_period_open_exception` from `246` ULP to `0` ULP (`0x405d4310bdc21078`) | `docs/articles/last-bit-pmt.md:47-59` |
| `CUMPRINC` / `CUMIPMT` `(nper - end_period) + 1.0` grouping (`12 / 12` vs `0 / 12`) | `docs/articles/last-bit-pmt.md:74-90` |
| Pre-oracle SHA-256 unseen `Holdout` (`5e2b6b35ad98075b...`, Excel `16.0.20527.20196` via Touchstone): `187 / 240` → `240 / 240` (`100.0%`); `308,710 / 308,710` exact across 5 functions with `0` changes on `20,205` sibling rows; `409 / 409` `dc-j9k.12` oracle cells; hardware x87 vs `wasm32` `soft87` bit-identical | `docs/articles/last-bit-pmt.md:94-118`, `crates/calc-func/witness-status.tsv` at commit `095bf1f1` |
| Open remainder `dc-j9k.91` (`~0.4%` odd halfway-tie edge in `x87_chains::atanh_tail`, `19 / 200` on tie-filtered `Probe 1` and `1 / 6,000` in `fx2`) | `docs/articles/last-bit-pmt.md:126` |
