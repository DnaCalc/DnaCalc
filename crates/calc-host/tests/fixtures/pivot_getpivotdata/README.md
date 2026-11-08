# GETPIVOTDATA oracle (M3; Excel 16.0.20527, 2026-11-08)

Bead dc-8wp.13 (plan package W3). `oracle.xlsx` is a workbook Excel saved after
`tools/excel-session/pivot-job.ps1` kind `m3` built 16 pivots on 15 sheets (one row field; two data
fields; row and column fields; two row fields in tabular and compact layout; dates; grouped dates; numeric
items; a report filter; a hidden item; no grand totals; blank item; Average; outline with subtotals; two
pivots side by side) over the `Source` sheet, and entered the formulas of the `GPD` sheet (column B; column
A is the case id).

- `oracle.capture.json` (in `../../captures/pivot_getpivotdata/`): the capture of the whole workbook by
  `cargo xtask capture --fixture ... --out ...` (123 formula cells, 0 capture failures). The pivots are
  not refreshed; their output cells are the literal cells Excel saved.
- `entries.tsv`: id, formula text Excel holds, kind (n number, e error code, empty), value, displayed
  text, entry refusal. 126 formulas were tried: 64 numbers, 59 errors (53 #REF!, 6 #N/A) and 3 that Excel
  refused to accept when typed (error 0x800A03EC; `args-one`, `args-five`, `args-zero`, so they are not
  in the oracle). The groups: names of the data field (caption, source name, case, trailing space), items
  and fields (case, trailing space, wildcard, numbers for text, errors as arguments, a cell as argument),
  totals (grand, row, column, subtotal, no grand totals), dates (serial, DATE, text forms), grouped
  labels, numeric items, filters and hidden items, references (outside, other sheet, range, inside, two
  pivots side by side, text address, INDIRECT, OFFSET), and the MDX forms (ordinary pivots give #REF!/#N/A;
  the data-model rows are in `../pivot_cube/`).
- Raw run: R2 `dnacalc-evidence/excel/pivot-m3-20261108/` (12 files, 697544 bytes, 0 differences).

Seen in the data (matching rules come only from these rows): item and field names match without case,
not with a trailing space; text dates match "2024-01-15" but not "1/15/2024"; dates match by serial and
DATE(); a numeric item matches the number or its text; a hidden item gives #REF!; a report-filter value
other than the current page gives #REF!; no grand total gives #REF! for the whole, row and column forms.
Every row of the grouped-date pivot (years, quarters) gave #REF!.
