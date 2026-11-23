# Noncompact row axes crossed with Month

The ww5 Windows lane authored these workbooks in Excel 16.0.20527 and captured
the TableRange2 cells on 2026-11-22. The plain workbook has six Region/Product
pivots across Month. The second workbook adds the same six layouts over a
source containing #N/A. The layouts cross tabular and outline form with Values
before, between and after the two source row fields.

The workbooks are the authored Excel saves. `cells.tsv` takes the nonempty
cells from `evidence/pivot-layout/excel.json` in the checked R2 archive
`dnacalc-evidence/excel/ww5-win64-20261122T154148Z/windows-evidence.zip`.
Columns are sheet, A1 address, value kind and value. Numbers use the captured
binary64 words. An absent address within TableRange2 denotes an empty cell.
