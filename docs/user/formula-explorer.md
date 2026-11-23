# DNA Calc - Formula Explorer: First Steps

**DNA Calc - Formula Explorer** (internally named `OneCalc`) is a single-formula workbench for entering one Excel formula at a time, supplying cell inputs, inspecting intermediate evaluation steps, and checking exact `binary64` values and formatting.

All behaviour described here is verified from the application code (`apps/dna-onecalc/`, `apps/dna-formula-ui/`), the site pages (`docs/site/pages/onecalc.html`, `docs/site/pages/addins.html`, `docs/site/pages/vba.html`), the draft release notes (`docs/site/release-notes/next.md`), and the Function Bank records (`crates/calc-func/witness-status.tsv`, `docs/fun/function-bank-data.js`). This document is a draft for owner review and is not yet published on the site.

> **Where it runs:** Formula Explorer is embedded live on the Home (`/`), Playground (`/play/`), and Formula Explorer (`/onecalc/`) pages, served standalone at `onecalc.dnacalc.com`, and packaged for Windows x64 desktop (`dna-onecalc-desktop.exe`) with native `.xll` and `.xlam` add-in hosting.

---

## 1. What Formula Explorer shows

Formula Explorer calculates through `calc-fml` and `calc-func` directly without a multi-formula workbook dependency graph. Every panel updates live as you edit:

| Area | Control / Panel | What it shows and does |
|---|---|---|
| **Header** | **Theme** (`System theme`, `Light`, `Dark`, `High contrast`) | Switches Strand theme (`dna-theme` in `localStorage`). |
| **Add-ins** | **Add-ins · `<n> loaded`** | On the web, displays *"Add-ins load in the desktop app; this page runs without a host that can load them."* On Windows x64 desktop, loads `.xll` and `.xlam` add-ins in a surrogate process with **Stop add-in process** / **Restart add-in process**, SHA-256 hashes, and registered functions. |
| **Session file** | **Session file · `Unsaved changes` / `Unchanged…`** | **New session**, **Open…**, **Save…**, **Save as…** (desktop), recent files (desktop), drag-and-drop, and built-in **Examples** (`SUM & blank input`, `Array`, `LAMBDA`, `Missing input`). Opens/saves constrained single-sheet `.xlsx` / `.xlsm` (plus `.xls`, `.xlt`, `.xla` up to `4 MB`). |
| **Macro bar** | **Macro security** (`Off`, `Guarded`, `Unguarded`) | Appears when opening a macro file; holds VBA until you click **Enable macros** or **Unblock and enable**. |
| **Formula bar** | Formula field + **`entered in <cell>`** | Syntax-highlighted editor with bracket-pair matching, function autocomplete, parameter signature help, `F1` **Function help**, `Ctrl+Shift+Enter` legacy array entry (`{=...}`), and anchor cell selector (default `H1` on `Sheet1`). |
| **Result** | **Result** + status badge | Shows state (`Ready`, `Calculated`, `Worksheet error`, `Unknown name or function`, `Missing input`, `Unknown function`, `Formula syntax`, `Not modelled`), formatted value, type badge (`number`, `text`, `logical`, `error`, `array`), **Exact value** disclosure when formatted/rounded, scrolling array grid (up to `50,000` cells / `10,000` rows × `200` columns), diagnostics, and **Calculation details** (`Raw result`, `Provenance`, `Environment`, and `Reads`). |
| **Inspect & trace** | **`F9`** + **Evaluation trace** | Select any formula sub-expression and press `F9` to replace it with its evaluated literal (`Escape` restores). Click **Trace the formula** and use **Previous step** / **Next step** to walk sub-expressions innermost-first with editor span highlights. |
| **Formatting** | **Effective display**, **Format & locale**, **Formatting**, **Conditional formatting** | Choose locale (`en-US`, `en-GB`, `en-ZA`, `de-DE`, `fr-FR`), number format code, `1900` / `1904` date system, cell font/fill/alignment/borders, and up to 13 conditional-format rule types (`Cell value`, `Text`, `Blanks`, `Errors`, `Dates`, `Formula`, `Top or bottom`, `Above or below average`, `Duplicate values`, `Unique values`, `Colour scale`, `Data bar`, `Icon set`) with priority order and `Stop if true`. |
| **Inputs** | **Inputs (`Sheet1!A1:H12`)** | Collapsed until the formula names a cell or uses Point mode. A `12` × `8` scratch grid of Excel constants (numbers, `%`, `TRUE`/`FALSE`, errors like `#N/A`, `'`-prefixed text, or plain text). Typing a `=...` formula into a grid cell is refused because multi-formula dependency order requires DNA Calc. |
| **VBA** | **VBA functions** | Standard-module editor (pre-seeded with `Module1` example) with **Load module** / **Unload module** and a strict values-only ceiling (writing to cells raises VBA error `1004`; `As Range`, `Caller`, and `ThisCell` return typed refusals). |
| **Catalogue** | **Functions** | Live filter across `calc-func`'s worksheet functions by name or description (shows up to `60` matches; click a signature to insert it). |

> **Screenshot placeholder:** Formula Explorer workspace with the formula editor at the top left, Result and Evaluation trace below it, and the Inputs scratch grid (`Sheet1!A1:H12`) and Functions catalogue on the right.

---

## 2. Worked example

### Part A: Formula with inputs, `F9`, and Evaluation trace

1. **Load the starter example:** Click **Try SUM with two inputs** in the empty Result panel (or expand **Session file → Examples** and click **SUM & blank input**).
   - Formula Explorer sets the formula to `=SUM(A1:A3)` in `H1`, automatically opens **Inputs**, puts `1.5` in `A1` and `2` in `A2`, and leaves `A3` blank.
   - **Result** displays state `Calculated`, type badge `number`, and value `3.5`.
2. **Add a nested operation:** Edit the formula to `=SUM(A1:A3)*2`. **Result** updates live to `7`.
3. **Evaluate a part with `F9`:** Select `SUM(A1:A3)` in the formula field and press `F9`.
   - The editor replaces `SUM(A1:A3)` in place with `3.5` (showing `=3.5*2`), keeping the evaluated part selected.
   - Press `Escape` to restore `=SUM(A1:A3)*2`.
4. **Walk the Evaluation trace:** In **Evaluation trace**, click **Trace the formula**, then click **Next step**:
   - Step 1 highlights `A1:A3`.
   - Step 2 highlights `SUM(A1:A3)` and shows its value `3.5`.
   - Step 3 highlights `SUM(A1:A3)*2` and shows `7`.
5. **Inspect Calculation details:** Expand **Calculation details** under **Result**:
   - **Raw result:** `number 7`
   - **Provenance:** `calculated in Sheet1!H1 by calc-fml's evaluator with calc-func's functions; 1 area read (3 cells)`
   - **Reads:** `Sheet1!A1:A3: Sheet1!A1 = number 1.5; Sheet1!A2 = number 2; Sheet1!A3 blank`

> **Screenshot placeholder:** Formula Explorer showing `=SUM(A1:A3)*2` at Step 2 of Evaluation trace (`SUM(A1:A3) = 3.5`), Inputs open with `A1 = 1.5` and `A2 = 2`, and Calculation details expanded.

### Part B: Exact `binary64` display and dynamic arrays

- **Exact `binary64` inspection (`=0.1+0.2`):** Enter `=0.1+0.2` in the formula field. **Result** shows `0.3` (Excel's `General` display at standard width) and reveals the **Exact value** disclosure, which expands to show `exactly 0.30000000000000004`—the shortest round-trip decimal of the computed IEEE 754 `binary64` result.
- **Spilling array (`=SEQUENCE(3,2)`):** Expand **Session file → Examples** and click **Array**. **Result** switches to type `array`, `3 x 2 array`, and renders the `3` × `2` scrollable grid (`1`–`6`). Open **Conditional formatting** and add a **Colour scale** or **Data bar** rule to see rules apply across the spilled array cells.

> **Screenshot placeholder:** Formula Explorer Result panel for `=0.1+0.2` with the `Exact value` disclosure expanded to show `exactly 0.30000000000000004`.

---

## 3. The bit-exact status

Formula Explorer computes with the same `calc-fml` and `calc-func` crates as DNA Calc and compares floating-point outputs against Excel **bit for bit** (`0` units in the last place across all 64 bits of an IEEE 754 `binary64` value, plus type, error code, and array shape).

- **What Formula Explorer shows inline:** Whenever `General` or a custom number format rounds or alters a number's display, **Exact value** exposes the exact shortest decimal representation of the computed `binary64`. Like DNA Calc, Formula Explorer calculates locally and does not query Excel live per formula.
- **Function Bank (full bank, `crates/calc-func/witness-status.tsv`, `2026-11-23`, commit `6f41b191`):**
  - **`4,780,049` of `4,794,410`** recorded Excel 16.0 answers matched (**`99.70%`**) across **`506` functions**.
  - **`430` functions are exact (`0` ULP)** on every recorded input.
  - Remaining non-matches across the other `76` functions: `10,658` last-bit (`1` ULP, `38` functions), `3,633` numeric (`31` functions), `44` gross (`5` functions), `0` structural, `0` refused, and `26` excluded (`2` functions).
- **Pinned Function Bank (`crates/calc-func/witness-status.tsv`, `2026-11-23`):** **`40,715` of `40,872`** matched (**`99.62%`**) across **`284` functions** (`264` exact).
- **Playground exhibit snapshot (`docs/fun/function-bank-data.js`, `2026-11-23`):** **`4,768,604` of `4,794,724`** matched (**`99.46%`**) across `506` functions (`436` exact in that snapshot).
- **Windows x64 add-in and VBA acceptance (`docs/site/pages/addins.html`, `docs/site/release-notes/next.md`):** **`350 of 350`** Formula Explorer XLL cases (`m4`), **`12 of 12`** `xloil-utils-sql`, **`109 of 109`** each on `simple-xlam` and `simple-xlsm`, **`22 of 22`** `dnaexamples-xlam`, and **`23 of 23`** `dnaexamples-xlsm`.

To check any specific function's match count, worst ULP miss, or recorded argument landscape, open **The Function Bank** (`/fun/function-bank.html` via `/play/`), search the **Handbook of Excel Functions** (`/handbook/` → `https://handbook.dnacalc.com/catalogue/index.html`), or read `/status/` and `/articles/last-bit-status/`.
