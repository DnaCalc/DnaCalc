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
  7 malformed cells (a formula inside an array formula's area), and the 58 OLE2 files. 9,220 files have formulas, 20,746,884 formula cells in all, 13,584,088 of them
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
