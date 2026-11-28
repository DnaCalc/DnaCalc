# Two-axis Product order

The synthetic workbooks and answer tables are DNA Calc test fixtures under the workspace MIT licence.

Six synthetic PivotTables workbooks captured by ww12 on Excel 16.0.20625.20108, Windows x64, AMD
Ryzen 7 4800H, 2026-11-24. Source and refresh observations are in `ww12/product/pivot.json`,
transferred in ww25's `pivot-product.zip` (SHA-256
`0ec43bb42c533c792aec63d943ce529f0b70882e980708104cdfa5a99bb10532`). Each workbook contains the
same factors arranged under different source row orders and labels. Each corresponding TSV holds
all refreshed TableRange2 cells for Product alone, Product with Sum of another field, and Product
with StDev of the same field. Numeric readings are captured binary64 words.

`ww10-precision.xlsx` is the original ww10 precision workbook from the same transfer, authored in
Excel 16.0 build 20625 on the same CPU on 2026-11-23. `ww10-precision.tsv` holds the 1,014 cells from the live
refresh observations in `ww10/pt1/precision-fixed/result.json`.
G6Forward and G6Reverse vary source order around large cancellations, missing and text-only groups,
and overflow. The replay counts nine extreme Product pivots as DNA Calc refusals under dc-8wp.10.
