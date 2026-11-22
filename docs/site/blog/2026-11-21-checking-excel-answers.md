# How DNA Calc checks formulas against Excel
Tags: Excel parity, testing, function bank, last bit
Status: draft

Excel workbooks usually store saved values (`<v>`) alongside their formulas. A viewer that only reads those saved values looks accurate until someone edits an input—or tests a dynamic array, an error, or a subtle rounding boundary.

DNA Calc treats **provenance before value** as a hard rule. A cell counts as a match only after the engine actually calculates it and the verdict checks value, type, error code, and spill shape against Excel's answer. A refused formula—where the engine does not yet model a feature—always counts as a failure in every pass rate, never skipped or hidden in the denominator.

## Two measurements: whole workbooks and the Function Bank

We track Excel parity on two complementary scales:

1. **Oracle workbooks (`cargo xtask corpus --oracle`):** Excel-saved `.xlsx` and `.xlsm` workbooks testing cell references, dependency order, dynamic-array spills, and workbook settings. The release baseline in `docs/plan/RELEASES.md` (`6847cc02`) stands at `77,036` of `78,264` formula cells (`98.43%`, 62 workbooks), and the latest run reported in `docs/articles/last-bit-number-text.md` (`86427f7a`) reached **`78,161` of `78,296` formula cells (`99.83%`, 63 workbooks)**.
2. **The Function Bank (`crates/calc-func/witness-status.tsv`):** Individual worksheet functions judged bit-for-bit against recorded Excel 16.0 outputs stored as 64-bit IEEE 754 hex words. At commit `acae3f80` (2026-11-22), the full bank covers **`506` functions and `4,794,193` Excel answers, matching `4,759,423` (`99.27%`)**, with **`424` functions exact** across every recorded input.

You can browse every function's daily progress, ULP distribution, and individual input landscapes in the Playground's **Function Bank** exhibit (`docs/fun/function-bank.html`).

## Asking Excel at 200,000+ calls a second with Touchstone

Matching Excel down to the 52nd fraction bit—zero units in the last place (ULP)—requires dense measurements, not guesses. We built **Touchstone** (`tools/excel-service/`), a broker driving multiple owned Excel 16.0 (`16.0.20527.20196`) processes on a private Windows machine over a socket (`200–270k` calls/s across three workers, `10,000` calls in ~`0.33 s`).

Our rules for using it are strict:

- We observe Excel's answers and consult public numerical-analysis literature; we never read or disassemble Microsoft's code.
- Before claiming a mechanism, we generate predictions on unseen inputs, commit their SHA-256 hash *first*, and only then ask Excel for the answers.
- We never special-case single inputs: a change only lands if it explains the underlying procedure and moves the whole bank and corpus forward without regressions.

## Three "last bit" breakthroughs from this week

This week's technical write-ups in `docs/articles/` show how that method works in practice:

- **`NORMSINV` (`docs/articles/last-bit-normsinv.md`):** Textbook normal-inverse approximations missed Excel by up to `581` ULP near `p = 0.5`. Sweeping windows of consecutive doubles uncovered four distinct regions, every step computing in x87 64-bit-significand precision (`PC = 64`) and rounding back to `binary64` on store. Identifying the Chebyshev rational interpolation at `T15` on `[0, 0.73]`, the `225/256` Padé expansion on `[0.925, 15/16]`, and Excel's stored `FYL2X` + `FSQRT` logarithm produced **`32,452 of 32,452`** frozen-holdout bit matches—taking all eight normal, lognormal, and confidence aliases to **`21,898 of 21,898`**.
- **The 12-function gamma family (`docs/articles/last-bit-gamma.md`):** Forward and inverse gamma functions (`POISSON`, `POISSON.DIST`, `GAMMADIST`, `GAMMA.DIST`, `CHIDIST`, `CHISQ.DIST`, `CHISQ.DIST.RT`, `GAMMA.INV`, `GAMMAINV`, `CHIINV`, `CHISQ.INV`, `CHISQ.INV.RT`) shared `3,584` misses across `162,739` rows. Dividing live `POISSON.DIST(k - 1, m, TRUE)` by `POISSON.DIST(k, m, FALSE)` cancelled the `500`-ULP saddle-point prefactor noise and exposed Excel's exact Modified-Lentz continued fraction (`TINY = 1e-30`), Goldstein's Algorithm 451 inverse start, and Halley stopping rule—cutting misses from **`3,584` to `784`** (`161,955 / 162,739`) and reaching `560 / 560` (`0` ULP) on forward frozen holdouts.
- **Number-to-text conversion (`docs/articles/last-bit-number-text.md`):** Behind `&`, `CONCAT`, `CONCATENATE`, `FIXED`, `DOLLAR`, `VALUETOTEXT`, and `TEXT`, one 15-digit routine (`first_precision`) had puzzled tests with ties never rounding up below `65,536` and a period-8 oscillation above `2^k`. It turned out to be an integer-split `Q16.64` path below `65,536` and a low-nibble-first x87 `PC = 64` scaling path with a truncating shift (`mv >> (-1 - ev)`) into a truncated `Q0.64` half-ulp table (`floor(2^63 / 10^k)`). A `32 / 32` frozen holdout on the four rare exponents where `ceil(frexp_k / 3.3219)` parts ways with `ceil(frexp_k * log10(2))` settled the exact integer formula, now running identically in pure `u64`/`u128` Rust across `x86_64`, `aarch64`, and `wasm32` (`CONCAT` `3,578 / 3,578`, `CONCATENATE` `3,133 / 3,133`, `FIXED` `3,783 / 3,783`, `VALUETOTEXT` `2,571 / 2,571`).

## Try it yourself—and what stays open

In Formula Explorer, try a spilling formula like `=SEQUENCE(A1,2)` (`A1=3`), then change `A1` to `=1/0` to watch value, type, and spill shape recalculate together—or test `=0.1+0.2` and expand **Exact value** to see why 15-digit display hides single-bit differences that the verdict catches.

Roughly `34,770` rows in the full bank still miss or are refused (for example, the `a < 1, x < 1.1` leaf in `GAMMADIST` under `dc-j9k.59`, `BETA.INV` edge cases, `TEXT` format-string/locale gaps under `dc-j9k.65` and `dc-0f1k.8`, and about `7,500` Enron text-to-number coercion cells under `dc-3k5n`). Every open remainder stays tracked against its owning bead until solved cleanly.
