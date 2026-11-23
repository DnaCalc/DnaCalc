# Short 1: Formula Explorer tour (`dnacalc.com`)

- **Status:** Draft (`dc-6q2k.14`)
- **Target length:** 2–3 minutes (~2:35)
- **Audience:** Developers, analysts, and spreadsheet authors landing on `dnacalc.com` who want to see what **DNA Calc - Formula Explorer** does in two and a half minutes.
- **The one thing shown:** Testing one Excel formula at a time in Formula Explorer (both in the browser at `/onecalc/` / `onecalc.dnacalc.com` and on Windows desktop for add-ins), inspecting intermediate values with `F9`, **Evaluation trace**, **Exact value**, and **Calculation details**, and reading DNA Calc's live bit-exact Excel status.

## Exact files and setup

- **Browser workbench:** `https://onecalc.dnacalc.com/` or the local site preview `/onecalc/` (`docs/site/pages/onecalc.html`, backed by `apps/dna-onecalc/src/surface.rs`).
- **Windows desktop add-in contrast (Shot 4):** `dna-onecalc-desktop.exe` with `X2Hello-AddIn64.xll` built from `crates/calc-xll-reference/fixture/X2Hello.csproj`.
- **Status pages (Shot 5):** `/status/` (`docs/site/pages/status.html`) and `docs/fun/function-bank.html` (backed by `crates/calc-func/witness-status.tsv`).

---

## Shot list and narration

### Shot 1 — What Formula Explorer is (0:00 – 0:25)

**On-screen actions**
1. Open `/onecalc/` (or `https://onecalc.dnacalc.com/`) in the browser.
2. Frame the `DNA Calc - Formula Explorer` header, the single formula bar, and the collapsed `Add-ins · 0 loaded`, `Session file`, `Inputs · no values entered`, and `VBA functions` sections.

**Narration**
> "When you want to check one Excel formula—why it spilled, why it rounded, or what an argument actually evaluates to—opening a full workbook is slow.
>
> This is **DNA Calc - Formula Explorer**. It's a formula-only workbench that runs our Rust Excel engine directly in your browser in WebAssembly—no uploads, no install—and on Windows desktop it also hosts native XLL and XLAM add-ins."

---

### Shot 2 — A real formula, `F9`, `Exact value`, and `Calculation details` (0:25 – 1:10)

**On-screen actions**
1. Click **Try SUM with two inputs** (`=SUM(A1:A3)` with `1.5` in `A1`, `2` in `A2`, blank `A3` → `Result`: `number` `3.5`).
2. Click into the formula field, enter `=ROUND(0.1+0.2, 2)`.
3. Highlight `0.1+0.2` inside the formula bar and press **F9** (replaces selection with `0.30000000000000004`), then press **Escape** to restore `=ROUND(0.1+0.2, 2)`.
4. Replace the formula with `=0.1+0.2`. Point to **Result** (`number` `0.3`), then click **Exact value** to expand (`exactly 0.30000000000000004`), and expand **Calculation details** (`Raw result`, `Provenance`: `calculated in H1 by calc-fml's evaluator with calc-func's functions; no cells read`, and the `1900` date system / locale line).

**Narration**
> "Let's try a real formula. Clicking `Try SUM with two inputs` puts `=SUM(A1:A3)` in the bar over `1.5`, `2`, and a blank cell, giving `3.5`.
>
> Now type `=ROUND(0.1+0.2, 2)`. Just like Excel, you can highlight `0.1+0.2` and press `F9` to evaluate that part in place—and `Escape` puts the formula right back.
>
> Type `=0.1+0.2` on its own and Excel's General format displays `0.3`, while `Exact value` reveals the full IEEE binary64 value underneath—`0.30000000000000004`. Under `Calculation details`, Formula Explorer also logs the raw result, which evaluator ran it, every cell read, and the active date system and locale."

---

### Shot 3 — Dynamic arrays and `Evaluation trace` (1:10 – 1:40)

**On-screen actions**
1. Under **Examples**, click **Array** (`=SEQUENCE(3,2)`) to show the **Array result** scrollable grid (`1, 2; 3, 4; 5, 6`).
2. Type `=SUM(A1:A3)*2` (with `1.5`, `2`, blank still in `Inputs!A1:A3`).
3. Click **Trace the formula** in the **Evaluation trace** panel, then click **Next step** to step from `SUM(A1:A3)` (`3.5`) to `SUM(A1:A3)*2` (`7`), showing the active sub-expression highlighted in the formula bar.

**Narration**
> "Dynamic arrays spill straight into a scrollable `Array result` grid—here `=SEQUENCE(3,2)`.
>
> And if a nested formula still surprises you, `Evaluation trace` walks the expression tree from the inside out, highlighting each sub-expression in the formula bar as you step from `SUM(A1:A3)` at `3.5` to `7`."

---

### Shot 4 — Values-only VBA, typed refusals, and the Windows add-in bench (1:40 – 2:05)

**On-screen actions**
1. Expand **VBA functions**, click **Load module** on the built-in `Module1.Add2` example, enter `40` in `A1`, and evaluate `=Module1.Add2(A1,2)` → `42`.
2. Briefly expand **Add-ins · 0 loaded** in the browser (`"Add-ins load in the desktop app; this page runs without a host that can load them."`), then cut for 3 seconds to Windows `dna-onecalc-desktop.exe` loading `X2Hello-AddIn64.xll` and evaluating `=SayHello("Excel-DNA")` → `"Hello, Excel-DNA"`.

**Narration**
> "You can even paste a VBA module and call `=Module1.Add2(A1,2)`. Because Formula Explorer is a values-only workbench, functions that need workbook context like `As Range` fail loudly with a typed refusal: `needs a workbook: open in DNA Calc`.
>
> In the browser, native add-ins are disabled by the sandbox; on Windows desktop, the same UI loads `.xll` and `.xlam` add-ins in an isolated surrogate process."

---

### Shot 5 — The bit-exact status (2:05 – 2:35)

**On-screen actions**
1. Switch to `/status/` on `dnacalc.com` (`docs/site/pages/status.html`), then to `docs/fun/function-bank.html`.
2. Show the Function Bank totals and hover a few green exact families (`PMT`, `NORM.S.INV`) and one amber/open family (`GAMMA.DIST` / `BETA.INV`).

**Narration**
> "Every answer Formula Explorer gives comes from the same engine we measure against Excel 16.0 bit by bit.
>
> Across our full Function Bank (`witness-status.tsv`), `4,779,590` of `4,794,193` recorded Excel answers match down to the last bit—`99.70%` across `506` functions, with `430` functions exact on every single recorded input. Every remaining mismatch stays counted in the denominator on our Status page and in the interactive Function Bank—so you can see exactly what matches today and what is still open."

---

## Facts relied on and sources

| Fact used in script | Exact source in repository |
|---|---|
| Public name `DNA Calc - Formula Explorer` (`OneCalc` internal), runs in browser Wasm (`onecalc.dnacalc.com` / `/onecalc/`) and Windows desktop (`dna-onecalc-desktop.exe`) | `docs/site/pages/onecalc.html`, `docs/site/release-notes/next.md` (`Formula Explorer`), `apps/dna-onecalc/src/surface.rs:1096` |
| **Try SUM with two inputs** loads `=SUM(A1:A3)` over `1.5`, `2`, blank → `3.5`; **Examples → Array** loads `=SEQUENCE(3,2)` | `apps/dna-onecalc/src/shown.rs:71-83`, `apps/dna-onecalc/src/surface.rs:855-862, 1331-1355` |
| `=0.1+0.2` renders `0.3` in Excel General format and `exactly 0.30000000000000004` under **Exact value** | `apps/dna-formula-ui/src/result.rs:228-249, 289-295`, `apps/dna-onecalc/src/surface.rs:880-885` |
| `F9` evaluates a selected sub-expression in place and `Escape` restores the formula; **Evaluation trace** steps innermost-first (`=SUM(A1:A2)*2`) | `apps/dna-formula-ui/src/formula_field.rs:746, 832-880`, `apps/dna-formula-ui/src/trace_panel.rs:77, 101`, `apps/dna-onecalc/src/trace.rs:176-190` |
| **Calculation details** shows `Raw result`, `Provenance` (`calculated in {cell} by calc-fml's evaluator with calc-func's functions; ...`), and `1900` / `1904` date system + locale | `apps/dna-onecalc/src/result.rs:11-28`, `apps/dna-onecalc/src/surface.rs:1562-1591` |
| **VBA functions**: `=Module1.Add2(A1,2)` with `40` in `A1` → `42`; `As Range` returns typed refusal `"needs a workbook: open in DNA Calc"` | `apps/dna-onecalc/src/surface.rs:1781-1801`, `docs/site/pages/vba.html:9`, `crates/calc-vba/src/lib.rs:77, 148-155` |
| Browser Add-ins note (`"Add-ins load in the desktop app; this page runs without a host that can load them."`) vs Windows desktop `X2Hello-AddIn64.xll` `=SayHello("Excel-DNA")` → `"Hello, Excel-DNA"` | `apps/dna-onecalc/src/surface.rs:1128-1130`, `docs/site/pages/addins.html:7`, `crates/calc-xll-reference/fixture/X2Hello.csproj` |
| Full Function Bank: `4,779,590` of `4,794,193` matched (`99.70%`), `506` functions, `430` exact (`0` ULP), `14,603` non-matches (`0` refused); pinned bank: `40,715` of `40,872` (`99.62%`, `284` functions, `264` exact) | `crates/calc-func/witness-status.tsv` at commit `095bf1f1` (`2026-11-23`; note `docs/fun/function-bank-data.js` holds the prior `4,768,604 / 4,794,724` snapshot until refreshed) |
| Refused / mismatched / missing answers all count as failures in the denominator | `docs/site/pages/status.html:7`, `AGENTS.md` (`Honesty rules`) |
