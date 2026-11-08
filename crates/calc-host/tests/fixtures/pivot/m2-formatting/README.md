# M2: pivot formatting (Excel 16.0.20527, 2026-11-08)

Captured by `tools/excel-session/pivot-job.ps1` kind `m2` (bead dc-8wp.11, plan package W2), in an owned
hidden Excel on a private desktop. Captures only: the Linux lanes implement against it (P5).

- `built.xlsx`: the workbook Excel saved after building every case (one pivot per sheet) over the
  `Source` sheet of `../m1-calculation/`.
- `formats.tsv`: 76 stages, 3330 cells. Columns: stage, address, kind, value, number_format (the displayed
  one), text, font_name, font_size, bold, italic, font_color, fill, pattern, borders
  (left|top|bottom|right as LineStyle:Weight:Color), halign, indent, column_width, row_height. All
  formatting is `Range.DisplayFormat` of each cell of TableRange2, i.e. what Excel shows after the table
  style, area formats and number formats are applied. Stages: `style-rows-<name>` and `style-cols-<name>`
  for 13 built-in styles (PivotStyleLight16, Light1, Light8, Light12, Light21, Medium1, Medium2, Medium9,
  Medium14, Medium28, Dark1, Dark6, Dark11) plus `style-none-*`; `option-medium9-*` and `option-light16-*`
  toggling row/column headers, row/column stripes and last column; `layout-*` (compact, outline, tabular,
  page filter, blank line) under Medium2; `numfmt-*` (eight data-field number formats on two fields, Show
  Values As percent with and without a custom format, count/average, source-formatted date);
  `area-*` (data field range, label range, item label, one data cell, whole data body, column and row
  range, `GrandTotalName`, header captions); and `refresh-<preserve|nopreserve>-<autofit|noautofit>` and
  `refresh-userwidth-*` with `-built` and `-refreshed` stages (a yellow data cell; a long product name and a
  large amount arrive; PreserveFormatting and HasAutoFormat toggled; a user column width of 30).
- Not committed: `result.json` (also the style option flags and field definitions; the style name read
  back as an object, so the style under test is the one in the stage name). R2 `dnacalc-evidence/excel/pivot-m2b-20261108/` (12 files, 5263080 bytes, copy
  checked, 0 differences). The first M2 run (`pivot-m2-20261108/`) lacked column widths and is superseded.

Seen: with HasAutoFormat on, a refresh resets column widths (a user width of 30 became 16.14) and sizes
column C from 6.29 to 7.29; with it off the user width stays. With PreserveFormatting off a refresh drops
the fill of a formatted data cell (65535 to 16777215); with it on the fill stays.
The default header texts are "Row Labels", "Column Labels" and "Grand Total".
