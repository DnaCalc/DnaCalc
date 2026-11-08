# CUBE oracle over an in-file data model (M4; Excel 16.0.20527, 2026-11-08)

Bead dc-8wp.17 (plan package K1). `pivot-job.ps1` kind `m4` loads the worksheet table `Table1`
(Region, Product, Amount; 6 rows) into the workbook's data model, adds seven measures (TotalAmount,
CountAmount, AvgAmount, MinAmount, MaxAmount, DistinctRegion, RowCount) and enters the CUBE formulas of
`tools/excel-session/pivot-m4-formulas.tsv` in `Sheet1!A` (the 108 rows of `../cube/measured.xlsx`, all
reproduced with the same values and error codes, plus 23 new rows: member properties, KPI member, rank 0,
unknown member and connection, slices).

- `oracle.xlsx`: the saved workbook (model part `xl/model/item.data` included). Capture:
  `../../captures/pivot_cube/oracle.capture.json`, 131 formula cells, 0 capture failures. It is the first
  capture of a workbook that calls the CUBE functions: the capture worker's data-model mode
  (`platform/calc-oracle-excel/worker/capture-worker.ps1`, `Test-DataModelOnly`) does not refuse them when
  the package declares the in-file model and no other connection, and waits for asynchronous queries after
  the rebuild; it never refreshes. Test: `cube_calls_over_the_in_file_data_model_are_not_refused`.
- `stages.tsv`: stage, address, kind, value, text, formula. Stages: `cube-entered`, `cube-calculated`, then
  the edit-then-refresh sequence (`Table1!V2` 10 to 100: `cube-after-edit-no-refresh`,
  `cube-after-connection-refresh`, `cube-after-refresh-all`; a table row added: `cube-after-add-row-no-refresh`,
  `cube-after-add-row-refresh`), the model pivot (`model-pivot`, 28 cells), `gpd-model` (11 GETPIVOTDATA
  rows in MDX form on it), `converted` (the pivot after Convert to Formulas: 36 CUBEMEMBER/CUBEVALUE
  cells), and the model-only table (worksheet table deleted: `cube-after-worksheet-table-deleted`,
  `cube-after-table-deleted-refresh`).
- `after-refresh.xlsx`, `model-pivot.xlsx`: the workbooks saved after the refresh sequence and with the
  pivot over the model.
- Raw run: R2 `dnacalc-evidence/excel/pivot-m4-20261108/` (17 files, 1003480 bytes, 0 differences).

Seen: the model is a snapshot. After the edit, TotalAmount stays 42.3 until `Workbook.RefreshAll`
(132.3); `Connections(ThisWorkbookDataModel).Refresh` alone did not change it; a table row added later
(South, D, 3) shows after the next RefreshAll (135.3, RowCount 7, set count 5). Deleting the worksheet table
leaves every value (the model keeps its copy). GETPIVOTDATA in MDX form on the model pivot gave numbers for
`[Measures].[TotalAmount]` with and without `[Table1].[Region]`/`[Product]` members (135.3, 105.2, 100.2,
120.2) and #REF! for captions ("Total"), a missing member, another measure not in the pivot and a member
name without `.&[...]`. CUBEMEMBERPROPERTY returned #N/A for MEMBER_CAPTION, MEMBER_UNIQUE_NAME,
MEMBER_NAME and MEMBER_KEY. Server OLAP is not measured (plan K5).
