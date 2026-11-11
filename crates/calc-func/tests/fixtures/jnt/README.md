# jnt: legacy Excel readings as an oracle workbook (dc-jnt)

`oracle.xlsx` is written by `../make-catalogue-fixtures.ps1` from the `sheet-jnt_*.txt` files here. It is a
workbook of its own, not part of the catalogue: an added catalogue sheet would change the context of
`prefix_metadata` and `dcuar_cell`, whose captures must stay valid.

Each non-comment line of a section file is one row: the formula, then up to six tab-separated inputs.
The inputs are written as constants to columns A, B, C, E, F and G of the line's row and the formula
to column D; `{A}`, `{B}`, ... in the formula stand for that row's cells.

| Input | Meaning |
|---|---|
| `n:<16 hex digits>` | a finite binary64 number, written with 17 significant digits |
| `s:<text>` | text (whitespace at either end is preserved) |
| `b:TRUE`, `b:FALSE` | a logical |
| `e:#N/A` | an error literal |
| `f:<formula>` | a formula cell |
| `-` | the cell stays empty |

**Text with control characters.** XML 1.0 cannot carry the characters U+0001 to U+001F (and a section
line cannot carry a tab or a newline). Such text, as an input or inside a text literal of a formula, is
built by Excel: `f:"ab"&CHAR(12)&"cd"` for an input cell, `("ab"&CHAR(12)&"cd")` inside a formula. The
answer recorded for such an input cell is Excel's own read-back from the live check, and the import
fails when that read-back is not the text of the legacy input. Text with U+0000, a lone surrogate, U+FFFE
or U+FFFF cannot be built this way and its rows are left out (each family's header comment counts them).

The answers are the legacy Excel readings (OxFunc, build 20430), imported by
`tools/excel-service/jnt_import.py` into `../../captures/jnt/sections/`; see its header.

**Environment.** The section captures carry the environment of Excel 16.0.20527 on this machine (user culture
en-ZA, its separators), copied from a catalogue live capture by `jnt_import.py environment` (run it after each
`cargo xtask capture --split`), with a calculation clock of 2026-11-09, the day of the live read-back. The
corpus harness builds the locale profile that number text needs from the environment: without it, a number
argument given "50%", "(2)" or "1/2" is refused as not modelled (189 rows). The answers are untouched.

**Hand rows (dc-vdma).** A row written by hand from a unit test (`tools/excel-service/jnt_hand.py`) is asked to the
question service only when the service can ask the formula the row holds. The service builds every number as a cell,
an array as `CHOOSE` over cells and an empty argument as a blank cell, and puts the formula in its own sheet
position, so a row with a number literal, an array constant, an omitted or blank argument, or a position or
reference function (ROW, COLUMN, ADDRESS, CELL, INDIRECT, OFFSET, ISREF, AREAS) takes its answer from the live
read-back of the sheet instead (`tools/excel-session/jnt-readback-job.ps1`, read by `jnt_import.py capture --live
<result.json>`): the formula is entered as written in its own cell. Excel refuses the entry of 18 formulas of the sheet jnt_aggregate_arrays and cannot open a file that holds one, so
`oracle.xlsx` stores those cells as their text (`entry-refused.txt` lists them; `make-catalogue-fixtures.ps1` writes
them) and the corpus judges each as the formula the capture records, by the loader's entry check. The file opens in
Excel as committed, and the job opens it read-only. A formula Excel refuses to enter is recorded as an `entry_rejected` observation (a claim about
Excel, which a DNA Calc rejection at entry agrees with).
