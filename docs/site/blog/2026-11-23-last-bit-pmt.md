# Why one extra rounding step decides Excel's PMT to the last bit
Tags: Excel parity, last bit, financial, x87
Status: draft

Ask a modern CPU to multiply two 64-bit `binary64` numbers and it rounds the 106-bit exact product straight to 53 significand bits. Ask an older x87 unit configured with 64-bit significand precision (`PC = 64`) and store the result back to `binary64`, and the product gets rounded **twice**—first to 64 bits in the x87 register, then to 53 bits on store.

Almost every time, both answers agree. Roughly once in `4,096` products (`2^-12`), though, the exact product sits just shy of a 53-bit halfway midpoint: rounding to 64 bits pulls it onto the exact tie, and round-to-nearest-even pushes it to the opposite neighbor.

## The insight: one unified `PC = 64` amortization path

Excel's five loan-amortization functions—`PMT`, `IPMT`, `PPMT`, `CUMIPMT`, and `CUMPRINC`—all share that x87 `PC = 64` store-each-step arithmetic. Before commit `b4bd656db`, `308,703` of `308,710` rows across our five-function witness bank already matched Excel (`291,978` of those for `PMT` alone), leaving just seven bank misses and one ignored `246`-ULP test in `IPMT`.

They turned out to come from three places where our code had strayed from Excel's single program:

1. **Double-rounding exponent products (`44 / 44` vs `0 / 44`):** Where `period_components` already stored `nper * L` from `PC = 64`, `pmt`, `end_payment_quotient`, and `cumulative_principal` still used plain `binary64` multiplication. On `cb19-probe1-0173` (`r ≈ 0.15025`, `nper ≈ 9.013`, `pv ≈ 72,555`, `type = 1`), `-(nper * L)` in `binary64` gives `0xc0c9d2d2c216746a`, while x87 `PC = 64` gives `0xc0c9d2d2c2167469`—matching Excel. Across `44` live double-rounding ties (isolating `last * L` and `count * L` with `nper = 65536`), `PC = 64` matched **`44 / 44`** against **`0 / 44`** for plain `binary64`.
2. **No special-case branches in `PMT`:** Deleting an `fv == -pv` branch and a `neg_log >= 0` plain-`binary64` fallback left one six-line `PC = 64` formula for every nonzero rate and period sign, clearing `PMT`'s lone bank miss (`round3-pmt-balance-0529`, `1` ULP) and avoiding spurious `#NUM!` overflows when `nper < 0`.
3. **One shared `PC = 64` logarithm:** Replacing a bespoke 12-term `atanh` polynomial with the shared `excel_log1p_times(1.0, rate)` (`|r| < 0.375` forward x87 `FST` loop, `FYL2X` above) shifted `alnrel(0.24042490731562907)` by one ULP—instantly dropping the ignored `246`-ULP `IPMT` test to `0` ULP and clearing all `3 / 3` `IPMT` misses (worst `24` ULP) and `3 / 3` `PPMT` misses (worst `2` ULP).

Live probes also confirmed that `CUMPRINC` and `CUMIPMT` group `last = (nper - end_period) + 1.0` (`12 / 12` vs `0 / 12` on fractional `nper`), the opposite order from `IPMT` and `PPMT`.

## What moved—and what stays open

- **Five-function witness bank:** `308,703 -> 308,710 / 308,710` (`100.000%` exact; `PMT` `291,978 / 291,978`, `IPMT` `4,411 / 4,411`, `PPMT` `4,387 / 4,387`, `CUMIPMT` `4,393 / 4,393`, `CUMPRINC` `3,541 / 3,541`), with `0` changes across the `20,205` rows of sibling functions (`FV`, `PV`, `NPER`, `ISPMT`, `RATE`).
- **Frozen suites and oracle corpus:** `240 / 240` (`100.0%`, up from `187 / 240`) on the unseen SHA-256-hashed holdout; `181 / 200` (up from `47 / 200`) on tie-concentrated `Probe 1`; all `409 / 409` original `dc-j9k.12` oracle cells and `78,344 / 78,390` (`99.94%`) across `156` oracle workbooks, bit-identical between hardware x87 and `wasm32` `soft87`.
- **Still open:** `dc-j9k.91` tracks the rare `x87_chains::atanh_tail` odd halfway-tie edge (`~0.4%` of random rates, causing the `19 / 200` misses in tie-filtered `Probe 1` and `1 / 6,000` in `fx2`), and parent `dc-j9k` tracks Windows `check --dependents`, browser `sweep`, and `perf`-profile parity checks.

[Read the full PMT and amortization article](/articles/last-bit-pmt/) (`docs/articles/last-bit-pmt.md`).
