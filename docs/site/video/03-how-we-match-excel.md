# Episode 3: How we know DNA Calc answers like Excel

- **Status:** Draft (`dc-6q2k.4`)
- **Target length:** 8–9 minutes
- **Audience:** Auditors, model reviewers, spreadsheet engineers, and anyone asking *"how do you actually know it gives the same answer as Excel?"*
- **The one thing shown:** DNA Calc's verification chain—why saved `<v>` caches don't count, how the oracle workbook corpus and the 4.79-million-row Function Bank are measured, how the Touchstone Excel answer service probes Excel 16.0 at 200k+ calls/s, and three concrete last-bit stories from this week (`NORMSINV`, the 12-function gamma family, and `Q0.64` number-to-text rounding).
- **Paired blog post:** [`docs/site/blog/2026-11-21-checking-excel-answers.md`](../blog/2026-11-21-checking-excel-answers.md)

## Exact files and setup

Prepare these browser tabs / terminal windows before recording:

1. **Playground exhibits (local site preview or `docs/fun/`):**
   - `docs/fun/function-bank.html` (and `#NORM-S-INV`, `#GAMMA-DIST`, `#GAMMA-INV`, `#CONCAT` to show landscapes and daily scrubber).
   - `docs/fun/dna-calc-brochure.html` (the binary64 bit explorer in section `A4 · Try it`, e.g. typing `0.1`).
   - `docs/fun/corpus-atlas.html` (real workbook distribution).
2. **Site Status page:** local site preview `/status/` (`docs/site/pages/status.html`).
3. **Repository evidence files (for brief on-screen reference or editor tabs):**
   - `crates/calc-func/witness-status.tsv` (506 full-bank functions, `4,759,423` of `4,794,193` matched as of `acae3f80`, 2026-11-22).
   - `tools/excel-service/README.md` (Touchstone service v2).
   - `docs/articles/last-bit-normsinv.md`, `docs/articles/last-bit-gamma.md`, `docs/articles/last-bit-number-text.md`.
4. **Live demo formula in Formula Explorer** (for Scene 1 & Scene 2):
   - Start with `=SEQUENCE(A1,2)` (`A1=3`), then change `A1` to `"bad"` or `=1/0` to show value, type, spill shape, and error propagation; also show `=NORM.S.INV(0.95)` or `=0.1+0.2` with **Exact value** open.

---

## Script

### Scene 1 — Provenance before value: why file caches don't count (0:00 – 1:15)

**Screen**
- Open `dna-calc` on `crates/calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx` (or Formula Explorer with `=SEQUENCE(A1,2)`).
- Point to the status bar (`calculated by DNA Calc's engine`) and show what happens when changing an input to `1/0` (`#DIV/0!`) or breaking a spill: value, type, and shape all update live.

**Narration**
> "Every Excel workbook already carries cached results in `<v>` tags alongside its formulas. If a spreadsheet viewer just prints those cached numbers, it looks 100% compatible—right up until you change an input.
>
> In DNA Calc, our first rule is *provenance before value*. A cell only counts as a match after our engine actually calculates it and our verdict compares the result—value, type, error code, and spill shape—against Excel's answer. And every formula cell in a workbook counts in the denominator. If we refuse a formula because we don't model a feature yet, that refusal counts as a failure in every statistic."

---

### Scene 2 — The Oracle Corpus and the Function Bank (1:15 – 3:15)

**Screen**
1. Switch to `/status/` on the local site preview (or `docs/fun/dna-calc-brochure.html`), pointing at the two headline cards:
   - Oracle workbook corpus baseline (`77,036` of `78,264` cells, `98.43%` across 62 workbooks in `RELEASES.md`, and the latest run after `nx8` in `docs/articles/last-bit-number-text.md` reaching `78,161` of `78,296` cells, `99.83%`, across 63 workbooks).
   - Full Function Bank (`crates/calc-func/witness-status.tsv`: `506` functions, `4,759,423` of `4,794,193` Excel answers matched, `424` functions exact across every recorded row).
2. Open `docs/fun/function-bank.html`:
   - Drag the daily history scrubber from late October to today to show functions appearing and turning green.
   - Open the ULP ladder / landscape gallery and click into `NORM.S.INV` (`#NORM-S-INV`) and `CHIDIST` / `POISSON.DIST` to show individual recorded answers stored as 64-bit IEEE 754 hex patterns.

**Narration**
> "We measure compatibility in two ways.
>
> First, whole workbooks—the oracle corpus. Those are real `.xlsx` and `.xlsm` workbooks saved by Excel, checking references, dependency order, dynamic-array spills, and workbook settings. In `docs/articles/last-bit-number-text.md`, that reached `78,161` of `78,296` formula cells across 63 workbooks—`99.83%`.
>
> Second, individual worksheet functions—the Function Bank. Right now `witness-status.tsv` tracks `506` functions and `4,794,193` recorded Excel answers, matching `4,759,423` of them (`99.27%`), with `424` functions exact across every single input we've ever recorded. Every answer is stored as its exact 64-bit double—all 16 hex digits—along with the Excel build that produced it. Close isn't the target; zero units in the last place is."

---

### Scene 3 — Asking Excel at 200,000 calls a second with Touchstone (3:15 – 4:45)

**Screen**
- Show `tools/excel-service/README.md` briefly (highlighting the Python client snippet: `excel.ask`, `consecutive_run`, `bisect_edge`, `200-270k calls/s` across three Excel 16.0.20527 workers).
- Switch to `docs/fun/dna-calc-brochure.html` section `A4 · Try it` (`Type a number: 0.1`, showing sign, 11-bit exponent, 52-bit mantissa, `1 ulp`, and Excel's 15-digit display).

**Narration**
> "To match Excel down to the 52nd fraction bit, you can't guess formulas by hand—you need to ask Excel a lot of questions.
>
> So we built Touchstone, an internal Excel answer service. Multiple Excel 16.0 processes run on a private Windows desktop, answering batched calls over a socket at over 200,000 function evaluations per second. It can sweep a million consecutive doubles in seconds or bisect a branch boundary down to a single bit.
>
> Our rule for using it is strict: we ask Excel for answers and read public numerical literature. We never read or disassemble Microsoft's code. And before we claim a fix works, we write out our predictions on unseen inputs, SHA-256 hash that prediction file, and *only then* ask Excel for the answers."

---

### Scene 4 — Three stories from this week's "last bit" work (4:45 – 7:45)

**Screen**
- Walk briefly through each of the three `docs/articles/last-bit-*.md` articles (or show the key equations/tables on screen alongside `function-bank.html`):
  1. **`docs/articles/last-bit-normsinv.md` (`NORMSINV`):** Show the four regions (`p < 0.075`, `[0.075, 0.925)`, `[0.925, 15/16]`, `> 15/16`), x87 `FYL2X` + `FSQRT` stored after each step, and the final result: `21,898 of 21,898` across the eight normal/lognormal/confidence aliases, plus `32,452 of 32,452` frozen predictions on the last upper-middle piece.
  2. **`docs/articles/last-bit-gamma.md` (12-function gamma family):** Show the cumulative table (`159,155 -> 161,955 / 162,739`, net `+2,800`, cutting misses from `3,584` to `784` across `POISSON`, `GAMMA.DIST`, `CHIDIST`, `CHIINV`, `CHISQ.INV`, etc.). Mention the trick: dividing `POISSON.DIST(k-1, m, TRUE)` by `POISSON.DIST(k, m, FALSE)` cancelled the shared saddle-point noise and exposed Modified Lentz with `TINY = 1e-30`.
  3. **`docs/articles/last-bit-number-text.md` (`Q0.64` number-to-text conversion):** Show why exact binary ties below `65,536` never rounded up and why numbers above `2^k` oscillated with period `8` (`mv >> 3` dropping `mv & 7` before adding truncated `Q64_HALF_POW10`), proved with a `32 / 32` frozen check on the four exponents where `ceil(k * log10(2))` and `ceil(k / 3.3219)` disagree—taking `CONCAT` (`3,578 / 3,578`), `CONCATENATE` (`3,133 / 3,133`), `FIXED` (`3,783 / 3,783`), and `VALUETOTEXT` (`2,571 / 2,571`) to exact, all in portable `u64`/`u128` Rust across x86_64, ARM64, and WebAssembly.

**Narration**
> "This week alone, three big puzzles fell using that method—all written up in `docs/articles/`.
>
> First, `NORMSINV`. Textbook approximations were off by up to 581 ULPs near one half. Mapping a million consecutive doubles revealed four distinct regions, each computing in x87 extended precision and rounding back to double after every multiply and add. Once we found the exact interpolation method, the `225/256` Padé expansion on the upper window, and Excel's `FYL2X` log store, frozen predictions matched `32,452 of 32,452` answers—taking all eight normal and lognormal inverse functions to `21,898 of 21,898`.
>
> Second, the twelve forward and inverse gamma functions—`POISSON`, `GAMMADIST`, `CHIDIST`, `CHIINV`, and friends. By dividing cumulative `POISSON.DIST` by adjacent point mass at the same mean, the 500-ULP prefactor noise cancelled out completely, exposing Excel's exact continued fraction (`TINY = 1e-30`), Goldstein's Algorithm 451 start, and Halley's stopping rule. Misses across the 162,739 gamma bank rows dropped from `3,584` to `784`, and we never special-case an input.
>
> Third, number-to-text formatting—what happens behind `&`, `CONCAT`, `FIXED`, and `TEXT`. We had bizarre rounding quirks at `65,536` and a period-8 oscillation above powers of two. Turns out Excel splits at `65,536` into an integer-plus-`Q0.64` fast path and an x87 nibble-scaling path with a truncating shift and a truncated half-ulp table. A 32-row frozen test even distinguished `ceil(k / 3.3219)` from `ceil(k * log10(2))` (`32 of 32`), and the whole thing now runs bit-for-bit in pure `u64`/`u128` Rust across Windows, Linux, ARM, and WebAssembly."

---

### Scene 5 — Honest remainders and closing (7:45 – 8:30)

**Screen**
- Return to `docs/fun/function-bank.html` showing the few remaining amber/red functions, or `/status/`.

**Narration**
> "We're honest about what's still open. Out of `4.79` million answers in the bank, about `34,770` still miss or are refused—things like the `a < 1, x < 1.1` leaf in `GAMMADIST`, some `BETA.INV` and `TEXT` format-string edges, and about 7,500 Enron text-to-number coercion cells in the broader corpus. Every single remainder stays tracked in the open issue list until a real mechanism explains it.
>
> You can explore every function and its landscape yourself in the Playground's Function Bank, or read the full detective stories in the blog and `docs/articles/`."

---

## Paired blog post outline (`docs/site/blog/2026-11-21-checking-excel-answers.md`)

1. **Provenance before value:** Why cached `<v>` values never count as passes and why refusals count as failures.
2. **Two gauges:** Oracle workbook corpus (`77,036 / 78,264` baseline in `RELEASES.md`, `78,161 / 78,296` after `nx8`) and the Function Bank (`506` functions, `4,759,423 / 4,794,193` matched in `witness-status.tsv`, `424` exact).
3. **Touchstone and frozen predictions:** Asking Excel 16.0.20527 at 200k+ calls/s, SHA-256 hashing unseen holdouts before asking, and never disassembling Microsoft code.
4. **Three recent breakthroughs (`docs/articles/`):** `NORMSINV` (`21,898 / 21,898`), the 12-function gamma family (`3,584 -> 784` misses), and `Q0.64` number-to-text conversion (`32 / 32` 4-binade proof, portable `u64`/`u128` Rust).
5. **What stays open:** Remaining function-bank rows, Enron text-to-number coercion (`dc-3k5n`), and how readers can inspect the Function Bank exhibit.
