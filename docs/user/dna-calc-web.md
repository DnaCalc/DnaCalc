# DNA Calc on the Web: First Steps

**DNA Calc** in the browser opens, edits, recalculates, and saves Excel workbooks locally in WebAssembly without uploading files to a server.

All behaviour described here is verified from the application code (`apps/dna-calc/`, `apps/dna-calc-worker/`, `apps/dna-formula-ui/`), the site header policy (`deploy/caddy/headers.caddy`), and the shipped site pages (`docs/site/pages/web.html`, `docs/site/pages/handbook.html`, `docs/site/pages/status.html`, `docs/site/release-notes/next.md`). This document is a draft for owner review and is not yet published on the site.

> **Platform note:** On macOS, `Cmd` (`Meta`) is accepted wherever `Ctrl` is listed below.

---

## 1. Opening DNA Calc in the browser

- **Where to launch it:** From `/web/` (**DNA Calc in the browser**), choose **Open local preview** (`/app/`) or follow the configured public app link (`https://app.dnacalc.com/` in `docs/site/layout.json`). You can also download `/sample.xlsx` (the repository's `d1.xlsx` fixture, whose `Calc` sheet uses `SEQUENCE` and demonstrates spilled arrays) to try right away.
- **Startup workbook:** On load, DNA Calc creates a blank workbook (`Book1.xlsx`), or instantiates your saved browser default `Book.xltx` / `Book.xltm` personal template if you have set one.
- **Local-only execution:** The site enforces `connect-src 'self'` (`deploy/caddy/headers.caddy`). Files you open are read in the browser and passed directly to `calc-host` inside a Web Worker; saved workbooks are generated locally and offered as browser downloads. No workbook data leaves your browser.

> **Screenshot placeholder:** Full DNA Calc web shell (`Book1.xlsx`) showing the masthead (`DNA Calc`, document name, `New`, `Open`, `Save`, `File`, `Add-ins`, `Undo`, `Redo`, `Calculate`, `Name Manager…`, `Format Cells…`, `Find…`, `Rules & review`, `Automatic`/`Manual` mode, `Settings & diagnostics`, and the six skin buttons), the Name Box and Formula Bar, the collapsed Result bar, the Classic Grid workspace, sheet tabs, and the status footer.

---

## 2. Opening a workbook and managing templates

- **Open (`Open` button):** Opens an Excel file from your device. The file picker accepts `.xlsx`, `.xlsm`, `.xlsb`, `.xltx`, `.xltm`, `.xls`, `.xla`, and `.xlt`. Opening a template (`.xltx` / `.xltm`) with **Open** creates a new unsaved workbook from that template.
- **Open a template to edit it (`File → Open for editing…`):** Opens an `.xltx` or `.xltm` file for editing the template itself rather than instantiating a new workbook.
- **Unsaved-changes protection:** Before **New**, **Open**, or loading a template replaces a workbook with unsaved changes, an active edit draft, or an unconfirmed download, DNA Calc opens **Keep your current work?** (`Keep working` / `Discard and continue`). Closing or reloading the browser tab while work is unsaved also triggers the browser's confirmation prompt.
- **Personal templates (`File → New from template`):** Store `.xltx` or `.xltm` files inside your browser's `localStorage` (`Add personal template…`) to **Create workbook**, **Insert worksheets** into the open workbook, or **Edit template**. You can also set a stored template as the browser default with **Use for new workbooks** (`Book`) or **Use for new worksheets** (`Sheet`), or clear them with **Clear defaults** / **Remove from personal templates**.
- **Macro-enabled workbooks:** Workbooks containing VBA (such as `.xlsm` or `.xltm`) open with macros disabled. A banner above the Formula Bar explains why the project is held and offers **Enable macros** (or **Unblock and enable** for files with an unknown/browser origin). You can also set the session policy (`Off`, `Guarded`, `Unguarded`) from **Macro security** in the footer. Enabling macros refreshes workbook VBA function completions in any open formula draft.

---

## 3. Editing cells and switching views

### Six skins over one workbook

DNA Calc renders six views (**skins**) over the same open workbook without reloading it. Switch using the masthead buttons or `Alt+1` … `Alt+6`:

| Shortcut | Skin | What it shows |
|---|---|---|
| `Alt+1` | **Grid** | Classic worksheet grid (`XFD1048576` bounds), in-cell editor, selection handles, panes, and sheet panels. |
| `Alt+2` | **Notebook** | Linked calculation blocks and narrative layout over worksheet sources. |
| `Alt+3` | **Atlas** | Workbook overview: sheets, used areas, formula/input counts, calculation timings, and refusals grouped by reason. |
| `Alt+4` | **Tree** | Foldable outline of defined names, sheets, rows, and cells with dependent counts. |
| `Alt+5` | **Canvas** | Precedent-to-dependent flow graph centred on the active cell (`[` and `]` step further back/forward). |
| `Alt+6` | **Ledger** | Filterable, sortable audit table of every formula cell, its value, provenance, and number format. |

Switching skins keeps your active cell, Undo/Redo history, and any in-progress formula draft (moving an active in-cell edit into the Formula Bar).

### Grid editing and workbook tools

In **Grid** (`Alt+1`), start editing with `F2`, double-click, typing directly into a cell, or clicking the Formula Bar. Use the Name Box (`Ctrl+G` / `F5`) to jump to cells or ranges, **Undo** / **Redo** (`Ctrl+Z` / `Ctrl+Y`), **Format Cells…** (`Ctrl+1`), **Name Manager…** (`Ctrl+F3`), **Find…** (`Ctrl+F`), **Rules & review** (conditional formatting rules, data validation audit circles, notes, threaded comments, and hyperlinks), and sheet tabs at the bottom (double-click to rename, right-click for menu, drag to reorder).

For the full list of Grid shortcuts, Point mode, Paste Special, Fill / Flash Fill, panes, AutoFit, and selection aggregates, see the [Classic Grid User Guide](classic-grid.md).

> **Screenshot placeholder:** Grid skin showing `sample.xlsx` (`Calc` sheet) with a spilled dynamic-array range outlined, the Result bar expanded to show provenance, and the six-skin switcher in the masthead.

---

## 4. Formulas, provenance, and calculation controls

- **Formula Bar assistance:** Both the in-cell editor and the Formula Bar use the shared formula field with syntax highlighting, bracket-pair highlighting, function completion (`Tab` accepts the highlighted function), and parameter signature help.
- **Point mode and `F4`:** Typing `=` in a blank cell arms Point mode (`F2` toggles between Point and text-caret editing) so arrow keys or clicking/dragging cells inserts cell and range references. Pressing `F4` cycles A1 and range anchors (`A1` → `$A$1` → `A$1` → `$A1`). `Ctrl+F2` moves an active edit between the cell and the Formula Bar.
- **Inspecting sub-expressions (`F9` and Evaluation trace):** While editing a formula, select any sub-expression and press `F9` to replace that part with its calculated literal (`Escape` restores the pre-`F9` formula). Below the active edit strip, the **Evaluation trace** panel offers **Trace the formula**, **Previous step**, and **Next step** to step through sub-expressions innermost-first while highlighting each span in the editor.
- **Calculation mode and iteration:** Use the masthead selector to switch between **Automatic** and **Manual** calculation, and click **Calculate** (`F9` in select mode) to recalculate on demand. Open **Settings & diagnostics** in the masthead to change **Theme** (`System theme`, `Light`, `Dark`, `High contrast`), enable **Iterate** for circular references (with **Maximum iterations** `1..=32767` and **Maximum change**), or inspect active-cell diagnostics, timings, and WebAssembly memory usage.
- **Result bar and provenance:** Expand the result disclosure bar below the Formula Bar (`result-details`)—which opens automatically on errors and refusals—to check:
  - **Value state:** `Calculated`, `File cache` (saved value from the opened file, not yet calculated), `Stale`, `Refused`, `Worksheet error`, `Literal`, `Empty`, or `Pending`. A file cache is never presented as a freshly calculated answer.
  - **Exact value (`exactly <number>`):** Shown beside a rounded `General` number display when the underlying `binary64` value has more digits than standard column display shows.
  - **Spills & errors:** Plain-language error meanings, **Open array anchor** from spilled children, **Open blocker `<address>`** on `#SPILL!`, and **Work package: `<bead>`** when a construct returns a typed refusal.

---

## 5. Saving and downloading

- **Save (`Save` button):** If the open workbook already has a file name and format, prepares the workbook package and starts a browser download. For a new unsaved workbook (`Book1.xlsx`), **Save** opens **Save As**.
- **Save As (`File → Save As…`):** Enter a **File name** and choose a **File type**:
  - **Excel workbook (`.xlsx`)**
  - **Excel macro-enabled workbook (`.xlsm`)**
  - **Excel template (`.xltx`)**
  - **Excel macro-enabled template (`.xltm`)**
- **Legacy and binary inputs (`.xls`, `.xlsb`):** Workbooks opened from `.xls` or `.xlsb` save as SpreadsheetML (`.xlsx` or `.xlsm` if macro-enabled).
- **VBA preservation guard:** If a workbook is macro-enabled and you select `.xlsx` or `.xltx` in **Save As**, the dialog warns that VBA would be lost and saving is refused if the file carries VBA or Excel macro sheets; choose `.xlsm` or `.xltm` instead.
- **Download confirmation:** After saving, the status line reports `Download started: <name>. Check your downloads; durable storage is unconfirmed.` and the footer displays `Download unconfirmed · check your files`. Verify the file is in your browser's Downloads folder before closing the tab.

> **Screenshot placeholder:** The Save As modal dialog showing the `File name` input, `File type` selector (`.xlsx`, `.xlsm`, `.xltx`, `.xltm`), macro-warning status line, and `Keep editing` / `Download` buttons.

---

## 6. What is not supported yet

### Browser-specific limits

- **Native XLL and COM add-ins:** Native `.xll` and `.xlam` add-in hosting is Windows x64 desktop only. In the browser, the **Add-ins** menu is disabled and reports: *"Add-ins (.xll, .xlam) load in the DNA Calc desktop app; this browser page cannot load them."*
- **Direct file-system overwrite and external network access:** Under `connect-src 'self'`, the browser app cannot read files outside what you choose in the file picker or overwrite files in place on disk; every save is a browser download.

### Open workbook and UI gaps across the preview

When DNA Calc encounters a feature it does not model yet, it returns a typed refusal naming the owning work package rather than guessing:

- **Workbooks with chart or dialog sheets:** Opening some workbooks containing chart sheets or dialog sheets is still incomplete (`dc-hc85`).
- **Text-to-number and date coercion:** Certain legacy coercion edge cases (tracked on Enron corpus workbooks under `dc-3k5n`) remain open.
- **Printing and drawings:** Page Setup controls (`dc-971p.57`) and print-preview shape CSP work (`dc-zjsg.12`) are still in progress.
- **Authoring gaps in Classic Grid:** Merge / Merge & Center / Unmerge authoring (`dc-971p.73`), hyperlink authoring, data-validation rule authoring, multi-sheet tab grouping, and the remaining grid differences listed in [Classic Grid §19](classic-grid.md#19-summary-of-differences-from-excel-and-owning-beads).
- **Non-exact function rows:** `76` functions in the full Function Bank (`crates/calc-func/witness-status.tsv` at `6f41b191`, `2026-11-23`) still have remaining `last_bit`, `numeric`, `gross`, or `excluded` rows (detailed in Section 7).

---

## 7. How to see a function's Excel-match status

Inside the spreadsheet UI, selecting a calculated cell deliberately reports *"Calculated by DNA Calc. Agreement with Excel has not been checked here."* (and **Atlas** states *"Agreement with Excel has not been measured in this view."*). Per-function Excel comparison status is published on the site alongside the app:

1. **Handbook of Excel Functions (`/handbook/` → `handbook.dnacalc.com`):** Search for any function (for example `SUM`, `ROUND`, `FILTER`, `SEQUENCE`, `XLOOKUP`) to open its page at `https://handbook.dnacalc.com/functions/<NAME>.html` (or structured JSON at `https://handbook.dnacalc.com/api/functions/FUNC.<NAME>.json` and recorded quirks at `https://handbook.dnacalc.com/notes/index.html`) showing behaviour, examples, quirks, and measured Excel evidence.
2. **The Function Bank (`/play/` → `/fun/function-bank.html`):** Interactive family map, daily progress timeline, worst-ULP ladder, argument-space miss landscape, and filterable conformance table for every function in the full bank. Each function shows its match count, worst ULP distance, and status class (`exact`, `last_bit`, `numeric`, `gross`, `structural`, `excluded`).
   - On `crates/calc-func/witness-status.tsv` (`2026-11-23`, `6f41b191`), the full bank covers **`506` functions** and **`4,780,049` of `4,794,410`** recorded Excel 16.0 answers (**`99.70%`**), with **`430` functions exact (`0` ULP)** on every recorded input.
   - In the Playground exhibit's `2026-11-23` snapshot (`docs/fun/function-bank-data.js`), the table lists **`4,768,604` of `4,794,724`** matched (**`99.46%`**) across `506` functions (`436` exact).
3. **Build status and technical articles (`/status/` and `/articles/`):** Shows the site's current Function Bank and oracle workbook corpus totals (`docs/plan/RELEASES.md`: `78,162` of `78,296` formula cells at `bb2f3d1b`; `docs/site/release-notes/next.md`: `78,167` of `78,296` = `99.84%`, `0` refusals), plus deep-dive articles such as `/articles/last-bit-status/`.
