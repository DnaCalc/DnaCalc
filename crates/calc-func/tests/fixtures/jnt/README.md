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
