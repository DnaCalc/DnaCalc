# Corpus classification with our reader (bead dc-zt3t, epic dc-ztvv)

Run on 2026-11-11 with `cargo xtask classify` (`tools/xtask-harness/src/classify.rs`). Counts only: no
workbook content, no sheet names. Per-file path, size and SHA-256 of the public sets are in
`docs/plan/corpus-manifests/`. The per-file JSON lines (`files.jsonl`) stay outside the repository.

**What the profile is for.** Excel answers are short today only because no capture environment exists
yet. The plan is an ongoing Excel evaluation environment that captures whole corpora (owner,
2026-11-11; at about 3 s per file, Enron is about 13 hours on one Excel process). So the profile's
lasting use is a coverage map and a stratifier. For each feature and each (function, argument shape),
it gives cells present and cells matched, so mismatch rates can be reported per feature and coverage
gaps are visible. Choosing files (`cargo xtask capture-select`) serves the first batches and a small
regression set. Capture and comparison tooling should be built for whole-corpus runs: sharded,
resumable, with results streamed out.

## The classifier

`cargo xtask classify <dir>... [--recursive] [--out <dir>] [--jobs <n>] [--no-host] [--keep-names]`
walks the folders with the corpus command's file walk (`corpus::folder_tree`), profiles every
workbook in two layers, and writes `files.jsonl` (one object per file) and `summary.md`.

* **Layer P, file facts, work on every file the container can be read from.** Format by file magic
  (OLE2 compound file, zip, text) and extension; size and SHA-256; the package's parts by category,
  with bytes; main content type; `docProps/app.xml` producer and version; workbook calculation
  properties (1904 dates, iteration, calculation mode, full calculation on load); sheets with kind,
  state and used range; every cell by type; every formula cell by kind (normal, shared, array, data
  table) with cached value and cached error counts; distinct functions with counts (shared-formula
  members expanded from their master); formula features (let, lambda, structured and spill
  references, 3-D and external references, array constants, implicit intersection, add-in
  `_xll.` calls, `_xludf` future or user functions, volatile functions, maximum nesting and length);
  defined names with scope, hidden state, built-in names, functions called and `#REF!`; data
  validation, conditional-format rule types, hyperlinks, merged cells, protection, drawings, controls,
  OLE objects, `ext` blocks by URI; VBA project size and a module count from the project stream
  (a heuristic; nothing is decoded). A file the container cannot be read from (an OLE2 `.xls`, a
  malformed zip, ZIP64) still gets format, size, hash and the refusal text.
* **Layer M, reader outcome, keyed by reader build.** The file is opened through the host route
  (`calc_host::HostState::open`, which is calc-doc's open and load). A refusal is a typed row (the
  same key `cargo xtask corpus` groups by); a success records loaded formulas, validations,
  conditional formats, merged areas, notes, cells held as not loaded (data tables, macro sheets,
  rich values, rejected formulas) and the reader's own `unsupported` notes. `reader.reader_build` is
  the repository revision. Reader outcomes are never mixed with the file facts.
* **Speed.** 16 threads, `--release` build: the 5,133 repository workbooks in 1.9 s (2,700 files per
  second); 4,652 EUSES `.xls` (container facts only) in 0.6 s; 3,331 library test files in 12 s
  (the large ExcelJS files dominate); 15,929 Enron files (1.9 GB, 20.7 million formula cells) in 302 s
  with 8 threads (53 files per second).
* Tests: formula analysis (functions, features, strings and quotes not counted, quoted 3-D, shared
  members), sheet scan, unreadable bytes, a blank workbook through the reader, argument parsing.

## What was classified

| Set | Files | Unique by SHA-256 | Bytes | Opened by calc-doc | Opened by host route | Files with formulas | Formula cells |
|---|---:|---:|---:|---:|---:|---:|---:|
| This repository's own fixtures (crates, platform, apps, tools) | 5,133 | 5,119 | 201.1 MB | 5,133 | 5,131 | 4,413 | 554,350 |
| EUSES (public) | 4,652 | 4,509 | 568.8 MB | 0 | 0 | not readable | not readable |
| Seven library test sets (public) | 3,331 | 3,200 | 165.1 MB | 2,458 | 2,405 | 392 | 49,910 |
| Enron spreadsheets (public) | 15,929 | 15,927 | 1,875 MB | 15,871 | 15,356 | 9,220 | 20,746,884 |

The Windows corpus folders (Ofgem, the 290 `local-ifc` workbooks, engine13 inputs) are on the
Windows machine, where no Rust is built; they are not in this table.

### The repository's own fixtures

All Excel 16 output. Formats: 4,188 `.xlsm`, 935 `.xlsx`, 10 `.xlsb`. 4,192 carry a VBA project
(3,992 modules by the stream heuristic), 3,979 the dynamic-array metadata part (3,973 files with
dynamic-array cells, 14,088 cells), 307 rich-data parts, 386 external links, 246 iterative
calculation, 3,960 manual calculation, 81 data tables. Most frequent functions: user-defined probe
names, SEQUENCE (2,105 files), RAND (1,250), SUM (874), ANCHORARRAY (589). 604 distinct functions.
Two files are refused by the host route (a malformed cell; unsupported metadata). Cells the reader
holds as not loaded: data table 173, macro sheet 9, rejected formula 8; three files give layout
warnings.

### Public sets

* **EUSES: 4,608 of 4,652 files are OLE2 compound files (`.xls`), 34 unknown, 10 text/html.** Our
  reader opens none: every one is `NotAZipArchive`. Nothing below the container is profiled until a
  `.xls` reader exists or Excel converts them.
* **Library test sets:** 2,421 `.xlsx`, 39 `.xlsm`, 36 `.xlsb`, 795 OLE2 `.xls`, 3 zip named `.xls`.
  (Counts for the host route in this paragraph and below are from the first run; the chart-sheet
  change of 2026-11-11 below moved 34 library files from refused to opened.) Refused by calc-doc: 11 malformed zips (central directory, local header or data descriptor
  mismatches), 4 with backslash entry names, 1 ZIP64, 9 `.xlsb` (two with chart sheets, three with
  pivot tables, four truncated records), plus the 795 `.xls`. Refused by the host route after
  opening: 34 chart or dialog sheets (`UnsupportedSheet`), 14 malformed cells, 9 workbook, 7 invalid
  parts and 23 others (bad indices, addresses, types).
* **Enron:** 15,871 `.xlsx` (the archive holds converted files; all say Microsoft Excel 15) and 58
  OLE2. Opened by the host route: 14,858 (93.6%) of 15,871 in the first run; 15,356 (96.8%) after the
  chart-sheet change (bead dc-hc85, rerun the same day with the same files). First run, refused: 505
  chart or dialog sheets (502 files have chart sheets, 75 have dialog sheets), 393 invalid parts, 108
  duplicate sheet keys, 7 malformed cells. After: 400 invalid parts (every one "a sheet's r:id names no
  relationship of the workbook part"; seven files were behind a chart sheet), 108 duplicate sheet keys,
  7 malformed cells (a formula inside an array formula's area), and the 58 OLE2 files. Bead dc-o41m
  (2026-11-11): all 15,871 `.xlsx` open by the host route (15,871 of 15,929 files, 99.6%; before
  15,356 = 96.8% of the `.xlsx`); the remaining 58 are OLE2 `.xls`. The 400 were `r:id=""` on
  687 veryHidden sheets (VBA modules; Excel opens them silently as empty sheets), the 108 repeated
  sheetIds are an Excel repair prompt, the 7 array files carry Excel's own `<f ca="1"/>` member
  marker; Excel's answers are in `crates/calc-doc/tests/fixtures/reader_tolerance/excel-answers.md`. 9,220 files have formulas, 20,746,884 formula cells in all, 13,584,088 of them
  shared-formula members; every one of those has a cached value.

## Common structures

Files that have it (Enron, 15,871 xlsx and 58 xls; libraries, 3,331 files). Libraries and
repository in the text above.

| Structure | Enron | Libraries |
|---|---:|---:|
| formula cells (files) | 9,220 | 392 |
| shared formulas | 5,622 files | 110 |
| external references in formulas | 1,807 files, 1.42 M cells | 14 |
| external link parts | 2,786 | 46 |
| volatile functions (NOW, TODAY, RAND, OFFSET, INDIRECT, CELL, INFO) | 2,276 files, 247,507 cells | 23 |
| defined names | 9,042 files, 145,411 names (30,571 with `#REF!`) | 278 |
| names that call functions | 436 (OFFSET 366, COUNTA 179, ROWS 90) | 14 |
| charts / chart sheets | 1,685 / 502 | 637 / 33 |
| pivot tables | 250 | 147 |
| embedded OLE objects, form controls, ActiveX | 723, 1,178, 230 | 5, 27, 5 |
| data tables (`TABLE()`) | 16 | 6 |
| add-in calls (`_xll.`) | 112 files, 107,138 cells | 1 |
| structured references | 121 | 52 |
| dynamic-array or spill | 0 | 13 (cm), 3 (ANCHORARRAY) |
| LET / LAMBDA | 0 | 2 / 0 |
| iterative calculation | 736 | 9 |
| manual calculation | 1,804 | 5 |
| 1904 date system | 33 | 27 |
| hidden / very hidden sheets | 888 / 448 | 23 / 7 |
| comment parts | 1,540 | 95 |
| conditional formatting | 473 | 143 |

Functions by number of files, Enron: SUM 6,745, IF 1,949, NOW 1,503, AVERAGE 1,023, VLOOKUP 857, ROUND
664, TODAY 550, SUBTOTAL 399, MIN 394, MAX 380, SUMIF 367, MONTH 352, CELL 339, YEAR 305; financial
functions in tens of files (NPV 93, PMT 85, XIRR 69, XNPV 53). 224 distinct functions in Enron, 266
in the library sets. Libraries: SUM 98, IF 41, SUBTOTAL 41, VLOOKUP 21, TEXT 18, ROUND 15.

## What our reader does not understand yet

1. **`.xls` (BIFF).** 4,608 EUSES, 795 library, 58 Enron files; no reader. This is the largest
   blocker for the public corpora, and the Excel conversion step (open, SaveAs `.xlsx`) is needed
   for them.
2. **Chart sheets and dialog sheets** (done, bead dc-hc85). A workbook with one opens: the sheet is an
   empty sheet of the model (positions, names, `SHEETS()` and sheet-scoped names count it), its part and
   the drawings and charts below it are kept as they were read, an untouched save and a save that edits
   other sheets leave them byte for byte, and a save that would edit, remove or move rows under one is
   refused (`SaveRefusal::PreservedSheetChanged`), as a macro sheet's change is. 505 Enron files and 34
   library files that this refused now open. Their charts are not read. Macro sheets load with their
   cells held as not loaded.
3. **Package faults that Excel tolerates:** 108 Enron files with a duplicate `sheetId`, 393 with an
   invalid part, 15 library zips with archive inconsistencies or backslash entry names, 1 ZIP64. The
   reader refuses all of them; Excel opens most.
4. **Cells the model holds as not loaded:** data tables (Enron 12 files, 1,731 cells; repository 173;
   libraries 402), rich values (15 library files, 28 cells), rejected formulas (4 cells in libraries,
   2 in Enron, 8 in the repository), macro-sheet formulas.
5. **Extension-only conditional rules and imported outline metadata** are preserved but cannot be
   displayed or edited (the reader's own notes: 15 library files and 44 Enron files).
6. **Binary workbooks (`.xlsb`)** convert to a package for 27 of the 36 library files; nine fail
   (truncated records, chart sheets, pivot tables).
7. External-link caches, add-in calls (`_xll.`, 112 Enron files), `OFFSET` in defined names (366
   files) and 1.4 million external-reference formula cells are read but need each owning bead
   (dc-ylr and others); the scan cannot say how many of them the engine evaluates.

## What this profile can and cannot say

* It describes files and the reader's outcome on them. No cell here has an Excel capture, so the
  "formula cells with a cached value" counts (Enron 20.7 M) are saved-cache facts; they are never
  passes. Captured and uncaptured cells have to be reported as separate populations.
* Function use is also keyed by argument shape (below); the type a referenced cell holds is not.
* The classifier is its own pass over the package for the file facts, because layer P must work on
  files the reader refuses; the reader outcome comes from the host route's one open. Folding the
  file facts into the capture census (`compare_inventory.rs`) is possible once the census can run on
  a file the reader refuses.
* The formula-feature scan is lexical (strings and quoted names skipped), not a parse.

## Argument shapes (bead dc-hc85)

`files.jsonl` has `function_shapes` per file: `NAME(tokens)` keys with call counts (shared-formula members
expanded), at most 400 distinct keys per file and the rest under `(other)`. The summary lists the distinct
(function, shape) pairs and the top 40. One token per argument, read from the formula text
(`tools/xtask-harness/src/formula_shapes.rs`): Touchstone's `n` number, `S` text literal, `B` boolean
literal, `e` error literal, `z` omitted, `aRxC` array constant; and for what a literal cannot be,
`r` one cell reference, `R` range, whole row or column or 3-D reference, `N` a name, `T` a structured
reference, `f` a function call's result, `x` any other expression. Touchstone's `s` and `b` (text and
boolean read through a cell) are `r`/`R` here, because the text does not say what the cell holds. A
trailing `!` marks functions whose answer depends on the formula's position or on the reference itself
(ROW, COLUMN, ROWS, COLUMNS, CELL, OFFSET, INDIRECT, ADDRESS, AREAS, FORMULATEXT, ISFORMULA, ISREF,
SHEET, SHEETS, SUBTOTAL, AGGREGATE, INFO, GETPIVOTDATA, HYPERLINK, INDEX, RTD); a capture of one runs on
the file route, never the call route. The census (`compare_inventory.rs`) is not where this fits: it
needs a file the reader opens and records no function; the classifier already scans every formula text,
including in files the reader refuses, so the shapes come from that one pass.

Enron: 1,199 distinct pairs (78 sensitive). The repository's own fixtures: 3,112 pairs. The first capture selection from these is `docs/plan/capture-batch-1.md`.

## Harv7 profile on kode-fi (2026-11-21, dc-ebb.3)


The supplied classifier was run from the repository on the unpacked sets with
`~/bin/xtask-harness classify <dir> --recursive --out <dir> --jobs 2`. Its SHA-256
starts `3a925a96d894f2c2`. Each run wrote a command, per-file JSONL and a summary
under `~/codex-runs/harv7/profile/`. The profile covers 60 current set profiles with 1 earlier or incomplete profiles (sheetpedia-xlsx).
Temporary hard links with standard Excel suffixes admitted existing add-ins and
templates to the classifier's filename filter. The FUSE VBA hard-link mirror is
counted with the original set. GovDocs1, source archives and R2-only workbooks
remain part of the cross-corpus internals census on `dc-ztvv.18`.


Macro detection read package directory entries or OLE storage and BIFF sheet
types. VBA and XLM targets were triaged with `~/vbawork/triage.py` and the
oletools venv. Standalone modules used the same script's source analyzer.
The public-corpus mode marks danger signals suspect. The harvested-set mode
also marks effect signals suspect. Unreadable targets remain unanalysed.
All reads were static.

### Per set

Workbook features count files with a feature. Dynamic arrays include spill
references, the storage marker and array-returning calls. For the generated
examples the marker itself is ignored because it is used for Formula2 encoding.
Macro outcome counts include standalone and embedded sources and XLM targets.
Zeros for a refused container describe the observed features. They do not
establish what is present inside that container.

| Set | State | Workbooks | CalcHost opened | Formula files | Dynamic array | LET / LAMBDA | GROUPBY / PIVOTBY | REGEX | Tables | Structured refs | Data tables | Pivots | Charts | Slicers | Names with functions | VBA projects | VBA modules | Clean | Suspect | Unanalysed |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `contextures-20261114` | current | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `creator-tips-local-20261121` | current | 564 | 559 | 283 | 46 | 2 | 0 | 0 | 26 | 12 | 1 | 12 | 47 | 2 | 11 | 223 | 111 | 251 | 83 | 0 |
| `creators-20261114` | current | 191 | 190 | 187 | 144 | 49 | 15 | 8 | 34 | 32 | 0 | 2 | 3 | 0 | 7 | 0 | 0 | 0 | 0 | 0 |
| `enron` | current | 15,929 | 15,921 | 9,262 | 0 | 0 | 0 | 0 | 0 | 119 | 16 | 250 | 1,733 | 0 | 436 | 0 | 0 | 0 | 0 | 0 |
| `euses` | current | 5,594 | 5,532 | 2,439 | 0 | 0 | 0 | 0 | 0 | 14 | 14 | 0 | 388 | 0 | 7 | 360 | 0 | 341 | 16 | 9 |
| `exceloffthegrid-local-20261115` | current | 438 | 436 | 199 | 144 | 72 | 18 | 2 | 237 | 117 | 0 | 13 | 29 | 15 | 32 | 52 | 0 | 45 | 7 | 0 |
| `fuse-vba` | current | 2,201 | 2,115 | 893 | 0 | 0 | 0 | 0 | 0 | 8 | 0 | 0 | 118 | 0 | 11 | 2,196 | 0 | 2,137 | 21 | 43 |
| `fuse-vba-full` | current | 13 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 12 | 0 | 7 | 0 | 5 |
| `github-feature-examples-20261114` | current | 3 | 3 | 3 | 2 | 1 | 0 | 0 | 3 | 1 | 0 | 3 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 0 |
| `github-financial-dashboard-20261114` | current | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 2 | 2 | 0 | 0 |
| `github-gist-tips-20261121` | current | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `github-licensed-20261114` | current | 989 | 944 | 300 | 30 | 10 | 0 | 0 | 75 | 52 | 1 | 32 | 43 | 10 | 15 | 136 | 2,405 | 1,842 | 699 | 0 |
| `github-licensed-tips-20261121` | current | 144 | 136 | 75 | 43 | 41 | 0 | 0 | 15 | 1 | 0 | 8 | 27 | 2 | 40 | 13 | 168 | 144 | 37 | 0 |
| `github-vba-20261114` | current | 2 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 2 | 0 | 1 | 1 | 0 |
| `github-vba-licensed-extra-20261114` | current | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 1 | 0 |
| `github-vba-tools-20261114` | current | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 1 | 0 | 0 |
| `ironcalc-20261114` | current | 267 | 267 | 257 | 74 | 15 | 0 | 2 | 2 | 1 | 0 | 1 | 3 | 0 | 6 | 0 | 0 | 0 | 0 | 0 |
| `libs-calamine` | current | 142 | 114 | 33 | 0 | 0 | 0 | 0 | 5 | 0 | 0 | 1 | 5 | 0 | 0 | 7 | 0 | 7 | 0 | 2 |
| `libs-closedxml` | current | 384 | 373 | 67 | 0 | 0 | 0 | 0 | 35 | 12 | 4 | 70 | 3 | 0 | 1 | 2 | 0 | 2 | 0 | 0 |
| `libs-excelize` | current | 9 | 5 | 2 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `libs-exceljs` | current | 37 | 31 | 7 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `libs-libreoffice` | current | 935 | 875 | 356 | 8 | 2 | 0 | 0 | 37 | 15 | 3 | 70 | 184 | 0 | 20 | 68 | 0 | 67 | 0 | 3 |
| `libs-poi` | current | 820 | 734 | 324 | 3 | 0 | 0 | 0 | 41 | 21 | 3 | 6 | 55 | 0 | 10 | 61 | 0 | 62 | 2 | 16 |
| `libs-xlsxwriter` | current | 1,005 | 1,005 | 37 | 3 | 0 | 0 | 0 | 38 | 8 | 0 | 0 | 432 | 0 | 0 | 5 | 0 | 5 | 0 | 0 |
| `mendeley-vba-20261114` | current | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 4 | 0 | 0 |
| `microsoft-20261114` | current | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `microsoft-afm-20261114` | current | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `microsoft-office-client-docs-20261121` | current | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 9 | 2 | 7 | 0 |
| `microsoft-officedev-20261114` | current | 3 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `microsoft-vba-docs-20261121` | current | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3,199 | 3,083 | 116 | 0 |
| `modern-lambda` | current | 11 | 11 | 11 | 11 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 11 | 0 | 0 | 0 | 0 | 0 |
| `new-github` | current | 2,858 | 2,699 | 831 | 182 | 28 | 1 | 4 | 323 | 151 | 3 | 200 | 205 | 24 | 66 | 755 | 0 | 270 | 486 | 2 |
| `new-zenodo` | current | 137 | 113 | 85 | 29 | 0 | 0 | 0 | 22 | 10 | 0 | 3 | 48 | 2 | 15 | 115 | 0 | 70 | 45 | 0 |
| `sestoft-finance2-20261114` | current | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `sheetpedia-xlsx` | earlier | 211,323 | 206,242 | 73,260 | 7,363 | 4,482 | 59 | 2 | 10,226 | 5,292 | 57 | 3,503 | 3,566 | 0 | 3,607 | 0 | 0 | 0 | 0 | 0 |
| `spreadsheetbench-v2-20261114` | current | 472 | 463 | 362 | 18 | 6 | 0 | 0 | 3 | 1 | 48 | 2 | 143 | 1 | 56 | 0 | 0 | 0 | 0 | 0 |
| `stackexchange-tips-20261121` | current | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2,585 | 2,274 | 311 | 0 |
| `vba-projects` | current | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 9 | 9 | 0 | 0 |
| `windows-captures-cache` | current | 347 | 347 | 27 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 6 | 322 | 0 | 289 | 44 | 0 |
| `windows-dc1yp-xlsb` | current | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `windows-dc5sr-dtc` | current | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |
| `windows-dc5sr-excel-open` | current | 82 | 82 | 43 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 82 | 0 | 70 | 12 | 0 |
| `windows-dc5sr-measure` | current | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `windows-dc5sr-ofgem` | current | 3 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0 | 0 | 0 |
| `windows-dc5sr-run1` | current | 595 | 595 | 461 | 57 | 28 | 0 | 4 | 30 | 20 | 0 | 0 | 0 | 0 | 12 | 123 | 0 | 105 | 18 | 0 |
| `windows-dc5sr-run2` | current | 601 | 601 | 467 | 61 | 28 | 0 | 4 | 36 | 28 | 0 | 0 | 0 | 0 | 12 | 123 | 0 | 105 | 18 | 0 |
| `windows-dc5sr-run3` | current | 376 | 376 | 298 | 32 | 18 | 0 | 2 | 18 | 18 | 0 | 0 | 0 | 0 | 10 | 66 | 0 | 60 | 6 | 0 |
| `windows-dc5sr-run4` | current | 595 | 595 | 461 | 61 | 28 | 0 | 4 | 36 | 28 | 0 | 0 | 0 | 0 | 6 | 123 | 0 | 105 | 18 | 0 |
| `windows-dc5sr-run5` | current | 601 | 601 | 467 | 61 | 28 | 0 | 4 | 36 | 28 | 0 | 0 | 0 | 0 | 12 | 123 | 0 | 105 | 18 | 0 |
| `windows-dc5sr-run6` | current | 128 | 128 | 48 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 125 | 0 | 108 | 20 | 0 |
| `windows-dc5sr-run7` | current | 128 | 128 | 48 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 125 | 0 | 108 | 20 | 0 |
| `windows-dc5sr-run8` | current | 128 | 128 | 48 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 125 | 0 | 108 | 20 | 0 |
| `windows-dc5sr-twins` | current | 327 | 326 | 189 | 31 | 14 | 0 | 2 | 18 | 14 | 0 | 0 | 0 | 0 | 8 | 84 | 0 | 73 | 13 | 0 |
| `windows-dcxuw-corpus` | current | 3 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 |
| `windows-dcxuw-ifc` | current | 87 | 87 | 46 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 83 | 0 | 72 | 13 | 0 |
| `windows-dcxuw-macro-saves` | current | 3 | 3 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 0 | 2 | 1 | 0 |
| `windows-fidelity-2` | current | 84 | 84 | 44 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 82 | 0 | 70 | 12 | 0 |
| `windows-local-ifc` | current | 43 | 43 | 3 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 41 | 0 | 36 | 6 | 0 |
| `xlcalc-20261114` | current | 7,171 | 7,171 | 5,242 | 1,325 | 45 | 4 | 0 | 606 | 106 | 0 | 3,552 | 178 | 204 | 15 | 0 | 0 | 0 | 0 | 0 |
| `zenodo-replicator-dynamics` | current | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `harv7-examples` | current | 93 | 93 | 93 | 59 | 22 | 7 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 89 | 89 | 0 | 0 |
| **Unique by SHA-256** | | 252,668 | 247,029 | 95,754 | 9,700 | 4,887 | 105 | 43 | 11,847 | 6,065 | 149 | 7,708 | 7,107 | 263 | 4,419 | 4,524 | 8,567 | 11,153 | 1,913 | 72 |

### Embedded VBA projects by triage outcome

| Set | Embedded projects | Clean | Suspect | Unanalysed |
|---|---:|---:|---:|---:|
| `creator-tips-local-20261121` | 223 | 189 | 34 | 0 |
| `euses` | 360 | 339 | 16 | 5 |
| `exceloffthegrid-local-20261115` | 52 | 45 | 7 | 0 |
| `fuse-vba` | 2,196 | 2,132 | 21 | 43 |
| `fuse-vba-full` | 12 | 7 | 0 | 5 |
| `github-licensed-20261114` | 136 | 23 | 113 | 0 |
| `github-licensed-tips-20261121` | 13 | 11 | 2 | 0 |
| `github-vba-20261114` | 2 | 1 | 1 | 0 |
| `github-vba-licensed-extra-20261114` | 1 | 0 | 1 | 0 |
| `github-vba-tools-20261114` | 1 | 1 | 0 | 0 |
| `libs-calamine` | 7 | 7 | 0 | 0 |
| `libs-closedxml` | 2 | 2 | 0 | 0 |
| `libs-libreoffice` | 68 | 67 | 0 | 1 |
| `libs-poi` | 61 | 59 | 2 | 0 |
| `libs-xlsxwriter` | 5 | 5 | 0 | 0 |
| `mendeley-vba-20261114` | 4 | 4 | 0 | 0 |
| `new-github` | 755 | 269 | 486 | 0 |
| `new-zenodo` | 115 | 70 | 45 | 0 |
| `windows-captures-cache` | 322 | 278 | 44 | 0 |
| `windows-dc5sr-excel-open` | 82 | 70 | 12 | 0 |
| `windows-dc5sr-run1` | 123 | 105 | 18 | 0 |
| `windows-dc5sr-run2` | 123 | 105 | 18 | 0 |
| `windows-dc5sr-run3` | 66 | 60 | 6 | 0 |
| `windows-dc5sr-run4` | 123 | 105 | 18 | 0 |
| `windows-dc5sr-run5` | 123 | 105 | 18 | 0 |
| `windows-dc5sr-run6` | 125 | 105 | 20 | 0 |
| `windows-dc5sr-run7` | 125 | 105 | 20 | 0 |
| `windows-dc5sr-run8` | 125 | 105 | 20 | 0 |
| `windows-dc5sr-twins` | 84 | 71 | 13 | 0 |
| `windows-dcxuw-ifc` | 83 | 70 | 13 | 0 |
| `windows-dcxuw-macro-saves` | 1 | 0 | 1 | 0 |
| `windows-fidelity-2` | 82 | 70 | 12 | 0 |
| `windows-local-ifc` | 41 | 35 | 6 | 0 |

### Corpus-wide coverage on kode-fi

This table counts distinct SHA-256 values across the rows above. The earlier R2-wide
snapshot contains 607,119 distinct Excel-family files. Its container inventory is in
[corpus-sources.md](corpus-sources.md#cross-corpus-deduplication-2026-11-17-dc-ztvv17).
No counts here measure calculation parity or an Excel capture.

| Feature or outcome | Unique files |
|---|---:|
| Workbooks | 252,668 |
| Opened by CalcHost | 247,029 |
| Workbooks with formulas | 95,754 |
| Dynamic arrays, spill references or array-returning functions | 9,700 |
| LET or LAMBDA | 4,887 |
| GROUPBY or PIVOTBY | 105 |
| REGEX functions | 43 |
| Table parts | 11,847 |
| Structured references | 6,065 |
| What-if data tables | 149 |
| PivotTable parts | 7,708 |
| Chart parts | 7,107 |
| Slicer parts or caches | 263 |
| Defined names with functions | 4,419 |
| Embedded VBA projects | 4,524 |
| Standalone VBA modules | 8,567 |
| XLM-bearing files | 30 |
| Embedded VBA projects, clean | 3,666 |
| Embedded VBA projects, suspect | 809 |
| Embedded VBA projects, unanalysed | 49 |
| Clean under static triage | 11,153 |
| Suspect under static triage | 1,913 |
| Unanalysed or unreadable macro targets | 72 |

REGEX, GROUPBY/PIVOTBY, slicers and what-if data tables have thinner coverage
than the broader dynamic-array population. Package refusals and the 2003 XML
input limit the legacy feature count. Unanalysed macro targets stay out of the
clean pool. The archive census `dc-ztvv.18` owns the wider content snapshot,
and `dc-4j0e` owns legacy Excel reader coverage.

### New formula source files

These are lexical counts in snippet files and library modules held alongside the workbooks.

| Set | Formula source files | Dynamic array functions or spill refs | LET / LAMBDA | GROUPBY / PIVOTBY | REGEX |
|---|---:|---:|---:|---:|---:|
| `github-gist-tips-20261121` | 2 | 2 | 2 | 0 | 0 |
| `github-licensed-tips-20261121` | 134 | 88 | 130 | 3 | 7 |
| `stackexchange-tips-20261121` | 1,539 | 211 | 159 | 7 | 7 |
| `microsoft-afm-20261114` | 5 | 4 | 5 | 0 | 0 |

### Capture examples

All 93 generated workbooks opened through the host reader with 94 formulas.
The 89 generated VBA modules passed harvested-set triage. These figures measure
static checks. The expected answers are empty and Windows Excel capture will
supply them. See [the capture manifest](../../tests/corpus-examples/capture-manifest.tsv).

| Kind | Feature | Cases |
|---|---|---:|
| formula | GROUPBY and PIVOTBY | 7 |
| formula | LAMBDA and LET | 14 |
| formula | REGEX | 10 |
| formula | TRIMRANGE | 6 |
| formula | dynamic arrays | 26 |
| formula | lookup | 9 |
| formula | reference and coercion | 9 |
| formula | text arrays | 12 |
| vba | Excel range model | 15 |
| vba | arrays and collections | 10 |
| vba | control flow | 6 |
| vba | date and time | 6 |
| vba | numeric coercion | 16 |
| vba | strings and comparison | 22 |
| vba | variant and errors | 14 |
