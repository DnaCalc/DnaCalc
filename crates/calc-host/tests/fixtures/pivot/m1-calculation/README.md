# M1: pivot calculation (Excel 16.0.20527, 2026-11-08)

Captured by `tools/excel-session/pivot-job.ps1` kind `m1` (bead dc-8wp.9, plan package W1), in an owned
hidden Excel on a private desktop. Captures only: the Linux lanes implement against it (P4).

- `built.xlsx`: the workbook Excel saved after building every case (one pivot per sheet C1.., plus
  `MissingItems`), over the `Source` sheet (36-row table `SalesTable`: duplicate row, blank Region, blank,
  negative, zero and text Amount, 36 months of dates), `Errs` (error, text, boolean, blank, date and
  case-variant keys and values) and `Prec` (precision groups G1..G13).
- `cells.tsv`: 85 stages, 4114 cells. Columns: stage, address, kind (s string, n number, b boolean,
  e error as Excel's HRESULT code), value. A stage is one case id (`agg-all-amount`, `show-*`, `calc-*`,
  `group-*`, `values-rows-*`, `subtotals-*`, `blank-line-*`, `nodata-*`, `sort-by-*`, `err-*`, `prec-*`)
  or a step of the missing-items sequence (`missing-items-built`, `-refresh-default`, `-refresh-none`,
  `-refresh-added`, `-delta-removed-auto`, `-max-limit`). Every pivot's cells are read from TableRange2.
- Not committed: `result.json` (1.6 MB: also each pivot's field/item definitions, data-field
  calculation and base field, subtotal flags, bits of every double). R2
  `dnacalc-evidence/excel/pivot-m1-20261108/` (12 files, 5272131 bytes, copy checked, 0 differences).

What Excel refused (recorded, not hidden): calculated items with Average/StDev/Var ("Averages, standard
deviations, and variances are not supported when a PivotTable report has calculated items", case
`calc-item-avg-refused`; `calc-item-half-sum` is the Sum form) and grouping the Amount field, which holds
text ("Cannot group that selection", `group-number-amount`; `group-number-units` and `group-number-clean`
group clean numbers).

Precision probe (decision D6): in `prec-all-functions` Sum of ten 0.1 is 0.9999999999999999, and
1E16 + 1 - 1E16 is 0 in either record order (G2, G3), so Sum adds binary64 in record order. G6
(1000000000000000.3, -1E15, 0.7) gives 0.7: the first value behaves as 1E15. G5 (1E308, 1E308, -1E308)
Sum overflows to #NUM! (code -2146826252) although the running total would return to 1E308; G9
(1E154, 1E154) has Sum 2E154 and Product 1E308 but StDev, StDevP, Var and VarP are #NUM!. The Grand Total
row of that pivot is #NUM! in every column that adds a failed group.
