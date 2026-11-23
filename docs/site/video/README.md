# Video drafts (`docs/site/video/`)

Draft series plan and episode scripts for owner review (`dc-6q2k.4`), paired with the draft blog posts in [`docs/site/blog/`](../blog/). Nothing here is published.

## Style and ground rules

- **Excel-DNA video style:** direct, practical, screen-first. Open the tool, type a real formula, load a real add-in or workbook, and show what happens. No slide decks and no hype.
- **Reproducible demos:** every script names exact repository fixtures and builds so the owner (or anyone following the blog post) can run the exact same steps.
- **Honest claims (`SITE_PLAN.md` section 2):** every figure carries its date, scope, and denominator (failed = mismatched + refused + missing). Where a capability is still in progress or refused on a platform (for example XLL hosting on Linux/browser, signed Windows installers, or workbook events in `dc-1nbp.3`), the script says so plainly.

## Short 2–3 minute scripts for `dnacalc.com` (`dc-6q2k.14`)

- [`short-01-formula-explorer-tour.md`](short-01-formula-explorer-tour.md): Formula Explorer tour — what it does, a real formula (`=SUM(A1:A3)`, `=ROUND(0.1+0.2, 2)`, `=0.1+0.2` with `F9`, `Exact value`, `Calculation details`, and `Evaluation trace`), and the bit-exact status.
- [`short-02-last-bit-pmt.md`](short-02-last-bit-pmt.md): The last-bit story — cracking Excel's `PMT` amortization family (`308,703 -> 308,710 / 308,710`) with double-rounding tie probes, the `±0.375` `alnrel` probe, and frozen SHA-256 holdouts.
- [`short-03-calcvba.md`](short-03-calcvba.md): CalcVBA — VBA running outside Excel across VM3 and the Cranelift JIT, with a host-neutral `ObjectModel` / `HostServices` design and explicit macro security.

## Episode series drafts (`dc-6q2k.4`)

- [`SERIES_PLAN.md`](SERIES_PLAN.md): eight-episode series plan (audience, one thing shown, demo files, target length, readiness notes).
- [`01-dna-calc-on-windows.md`](01-dna-calc-on-windows.md): Episode 1 full script — opening a real workbook with VBA and an XLL add-in in DNA Calc on Windows, plus switching between the six skins over one workbook. Paired with [`../blog/2026-11-21-what-is-dna-calc.md`](../blog/2026-11-21-what-is-dna-calc.md).
- [`02-formula-explorer-addin-bench.md`](02-formula-explorer-addin-bench.md): Episode 2 full script — using DNA Calc - Formula Explorer as a test bench for an Excel-DNA add-in (`F1` help, `F9` part evaluation, Evaluation trace, Exact value, Inputs grid, typed refusals). Paired with [`../blog/2026-11-21-onecalc-addin-bench.md`](../blog/2026-11-21-onecalc-addin-bench.md).
- [`03-how-we-match-excel.md`](03-how-we-match-excel.md): Episode 3 full script — how we know DNA Calc answers like Excel (oracle workbooks, the Function Bank, Touchstone, frozen SHA-256 holdouts, and examples from `NORMSINV`, the gamma family, and `Q0.64` number-to-text conversion). Paired with [`../blog/2026-11-21-checking-excel-answers.md`](../blog/2026-11-21-checking-excel-answers.md).
