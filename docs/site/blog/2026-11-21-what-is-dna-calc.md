# What is DNA Calc?
Tags: DNA Calc, Windows, VBA, skins
Status: draft

DNA Calc is an open-source (MIT) spreadsheet and calculation engine written in Rust. Its goal is straightforward: open Excel workbooks, calculate them the way Excel does—down to the last bit of every 64-bit double—and run your VBA and XLL add-ins on Windows.

The same calculation engine powers three targets:

- **DNA Calc for Windows** (preview): a desktop spreadsheet that opens `.xlsx`, `.xlsm`, `.xlsb`, and `.xls` workbooks, enables workbook VBA on demand, and loads native x64 `.xll` and `.xlam` add-ins.
- **DNA Calc in the browser**: a WebAssembly build that recalculates workbooks locally in your browser tab without uploading files.
- **DNA Calc - Formula Explorer** (and the **`dnacalc` Linux runner**): a single-formula workbench for inspecting formulas and add-ins, plus a dependency-free CLI (`recalc`, `diff`, `get`, `set`, `run`, `serve`) for checking workbooks in scripts and CI.

## Open a real workbook and inspect its provenance

When you open a workbook such as the generated `d1.xlsx` fixture (`crates/calc-doc/tests/fixtures/cdoc_m1_2/d1.xlsx`), DNA Calc calculates dynamic-array formulas like `=SEQUENCE(3,2)` and `=SORT(...)`, structured table references like `=SUM(Sales[Units])`, `XLOOKUP`, and blocked spills (`#SPILL!`) directly in the engine.

The status line at the bottom of the window always tells you where the active cell's value came from—whether it was calculated by DNA Calc's engine or is still a saved value read from the file cache. A file cache never pretends to be a calculated result.

## Six skins over one workbook

In DNA Calc, the familiar spreadsheet grid is one *skin* among six views over the exact same open workbook:

- **Grid** — the worksheet grid with formula bar, sheet tabs, formatting, and spills.
- **Notebook** — linked calculation blocks and narrative view.
- **Atlas** — workbook overview of sheets, formulas, inputs, and any typed refusals.
- **Tree** — outline and formula dependency explorer.
- **Canvas** and **Ledger** — spatial and tabular views over the same model.

Switching skins re-projects the workbook without reloading it. Following the recent skin-internals cleanup (`dc-xfrhg`), all six skins read through one controller and shared worker projections, keeping your selection, undo/redo history, and active formula edits intact as you switch.

## Run VBA and load add-ins on Windows

Open a copy of `crates/calc-xll-reference/fixture/vba/DnaSimple.xlsm`. Just like Excel, DNA Calc holds the VBA project disabled until you click **Enable macros** (or adjust macro security between `Off`, `Guarded`, and `Unguarded`). CalcDoc keeps the raw VBA project storage intact on save, while CalcVBA parses and executes the modules—supporting functions like `=SimpleNeg(2.5)` (`-2.5`), array-returning functions like `=SimpleSeq(3)`, and `Application.WorksheetFunction` calls (`109 of 109` matching Excel across module, JIT, and `.xlsm` routes).

Native XLL add-ins load through the **Add-ins** menu on Windows x64. Loading `X2Hello-AddIn64.xll` (built from `crates/calc-xll-reference/fixture/X2Hello.csproj`) registers `SayHello` through `xlfRegister` and wires it straight into formula autocomplete so `=SayHello("Windows")` returns `"Hello, Windows"`.

## What is true today—and what is still ahead

The project measures every claim against Excel 16.0:

- **Oracle workbooks:** `77,036` of `78,264` formula cells (`98.43%`) in the `RELEASES.md` baseline (62 workbooks, commit `6847cc02`), and `78,161` of `78,296` (`99.83%`) across 63 workbooks in the latest `last-bit-number-text.md` run (`86427f7a`).
- **Function Bank:** `4,759,423` of `4,794,193` Excel answers matched (`99.27%`) across `506` functions in `witness-status.tsv` (`acae3f80`), with `424` functions exact on every recorded input.
- **Desktop page cases:** `729` of `732` matching Excel on the DNA Calc Windows page suite (`887ebb5e`).

Several desktop pieces are still in progress and listed openly: signed Windows installers (`dc-fk66.2`, `dc-fk66.3`), workbook events such as `SheetChange` and `AfterCalculate` for add-ins (`dc-1nbp.3`), drawings and charts (`dc-zjsg`, `dc-hc85`), printing (`dc-971p.57`), AutoFilter (`dc-971p.37`), and sort (`dc-971p.38`). When DNA Calc hits an unmodelled feature, it returns a typed refusal naming what is missing instead of guessing.

[Try DNA Calc - Formula Explorer in the browser](https://onecalc.dnacalc.com/)
