# M5: pivot structure, source and layout edits (Excel 16.0.20527, 2026-11-09)

Captured by `tools/excel-session/pivot-job.ps1` kinds `m5` and `open` (bead dc-8wp, plan package M5 and the Windows replays of
P7 and P8), in owned hidden Excel sessions on private desktops (guard: 0 windows on the owner's desktop). Alerts were off for the
`m5` scenarios and on for the `dlg-` ones; no dialog was ever answered and none appeared (see Refusals).

- `base-range.xlsx`, `base-table.xlsx`: the start workbook of every scenario, as Excel saved it. Sheet `Src`: header row 2,
  data rows 3-14, fields Region, Product, Amount, Units in B:E (12 rows; North/South/East by row, Alpha/Beta/Gamma/Delta by row),
  note cells `A1`, `H2`, `B18`. Sheet `Out`: pivot `pvt_m5` at `C4:H9` (Region on rows, Product on columns, Sum Amount),
  with markers `A1`, `C2` (above), `B5` (left), `J5` (right), `C12` (below). Range source `Src!R2C2:R14C5`; table source: table
  `TblSrc` over `B2:E14`. (A table named `T1` is read by Excel as the cell address T1, hence the name.)
- `scenarios.tsv`: one line per scenario (127): `act` (ok, or the Excel error text of the edit), `refresh`, `refresh_reopened`,
  `save`, then the pivot ranges and sources (`name@range@source`) and the table ranges after stages 0 built, 1 edited (no refresh),
  2 refreshed, 3 saved and reopened, 4 reopened and refreshed. Scenario ids: `out-*` edits around the output, `src-*` around a
  worksheet-range source, `tbl-*` around a table source (incl. growth), `rng-*`, `ovl-*` overlap, `lay-*` layout changes on an
  unrefreshed pivot (`lay-clean-*` have no markers near the output), `base-*`.
- `cells.tsv`: every non-empty cell of `Out` and `Src` (A1:P40) at stages 1 and 2 (0 for `base-*`): scenario, stage, sheet, address,
  kind (n, s, b, e), value. Error cells carry Excel's raw code (the collapse code below is -2146826243).
- Not committed: `result.json` per job and the saved workbooks. R2 `dnacalc-evidence/excel/pivot-m5-20261109/` (646 files,
  44348457 bytes, copy checked, 0 differences).

## What Excel does (all via COM, whole-row and whole-column edits)

Output `C4:H9`:
- Insert at or above the first row/column (rows 2 or 4; columns A or C): the output moves. Insert on a row/column after the last
  row/column (10, I): nothing. Insert on any row/column from the second to the last (5-9, D-H): refused.
- Delete wholly before the output: it moves. Wholly after: nothing. A delete that overlaps part of the output (first row,
  last row, a header row, across the top or bottom): refused. Deleting exactly all rows or columns of the output removes the pivot
  (no table remains; the sheet has no pivot).
- Refusal text (identical for insert and delete, rows and columns):
  `We can't make this change for the selected cells because it will affect a PivotTable. Use the field list to change the report. If you are trying to insert or delete cells, move the PivotTable and try again.`
  Writing a data cell: `We can't change this part of the PivotTable.` Writing an item label renames the item (it survives a refresh).
  Clearing the whole output removes the pivot.
- Partial-range shifts (`out-ins-cells-partial-down`, `out-del-cells-partial-up`) resize the output by one row without
  refusal; `Insert(xlShiftToRight)` of two cells inside the output crashed Excel (RPC unavailable), so those two
  scenarios (`blk-*`, `dlg-blk-*`) have no data. DNA Calc does not model partial-range shifts of an output (it refuses them).

Worksheet-range source `Src!R2C2:R14C5` follows ordinary reference rules, stored as text and not changed by a refresh:
- Insert row at/above the header (1, 2): the range moves down. Insert rows 3-14 (first data row to the last row): the range
  grows by one; the blank row refreshes as an item `(blank)`. Insert at 15: nothing. Columns alike (A, B move; D, E grow; F nothing).
- Delete above: moves. Delete the header row: the range keeps its first row and loses its last (`R2C2:R13C5`) and a refresh leaves
  the pivot as a one-cell error stub; a data row (first, inside, last) or rows across the end shrink the range (`R2C2:R11C5` for
  12-16); delete after: nothing. Deleting all data rows leaves `R2C2:R2C5`, and a refresh fails with `This command requires at
  least two rows of source data. You cannot use the command on a selection in only one row.` Deleting every source column or
  clearing the range leaves the range text and a refresh gives the one-cell error stub.
- Deleting the first or a used column and refreshing: the pivot loses that field (fields kept: `Product` + `Sum Amount` for
  Region deleted, `Region` for Product deleted); deleting an unused column changes nothing. Renaming a used header (`Product` to
  `Item`) and refreshing drops that field.

Table source `TblSrc`: the pivot keeps the table name; the table follows the edit (insert/delete rows and columns inside it, at its
header and boundaries as for any table: `tbl-*`). Typing in the row below the table or `ListRows.Add` grows the table to `B2:E15`
and a refresh picks the new row up (new key: the output grows; existing keys: the same range). `Resize` follows too. A new
column typed right of the header grows the table to `B2:F14`; deleting a pivot's field column drops the field on refresh.
Deleting the table's header row through the range is refused by COM (`tbl-del-row-header`).

Overlap (output growing into cells): a refresh, a layout change, `PivotTable.Location` or a pivot created onto nonblank cells
or another pivot's cells does not prompt, with alerts on or off. The growing pivot collapses to its top-left cell, which holds
an error (-2146826243); the pivot keeps its fields; the blocking cells are untouched (`ovl-nonblank-*`, `ovl-layout-*`,
`ovl-other-pivot-*`, `ovl-move-location-onto-nonblank`, `ovl-create-onto-nonblank`). `Location` onto another pivot and creating a
pivot over a pivot are refused: `We couldn't complete the action for the PivotTable "pvt_m5" in the sheet "Out" because there's already a PivotTable "pvt_other" there. Make space and try again.` and
`We couldn't insert your PivotTable at the selected location in the sheet.`

Layout change on an unrefreshed pivot (`lay-*`): the new layout is computed from the cache, not the edited source (the source edits
`E3`=1000, `D4`=West, `C4`=Zeta, `B3`=West do not appear until a refresh); a field that was not in the cache is refused
(`lay-source-column-added`). Orientation changes that need room collapse as above when markers are in the way.

## Replay of DNA Calc saves (P7, P8 phase one)

`tools/excel-session/pivot-job.ps1` kind `open` opens each file read-only and records new `%TEMP%\error*.xml` repair logs, the
workbook name, every pivot, every cell of A1:N80, then refreshes every pivot. The eight saves from the host test
`dc_8wp_15__saves_for_the_excel_replay_reopen_to_the_same_cells` (set `DNACALC_PIVOT_SAVES`) all open with no repair log, and
every value DNA Calc held matches Excel's opened value (1,026 of 1,026 cells; after Excel's refresh also, except the
layout-swapped pivot, where DNA Calc computes from the saved records and Excel from the edited source, as in D1). The first
replay found six of eight saves refused by `Workbooks.Open` (0x800A03EC): a shared-formula group the file did not hold whose
formula names another sheet (`GETPIVOTDATA("Sum of Amount",PvOne!$A$3)` in B2 and B20) is not accepted by Excel; calc-doc now
writes such formulas individually (the same group with a constant formula, or without a sheet reference, opens).

Second replay (P8 phase two): the 25 saves of the structural scenarios of `pivot_m5.rs` (output moved, sources moved or grown,
a refresh after each; `DNACALC_PIVOT_SAVES`) plus the eight above: three of the 25 (a row inserted inside the source, which
refreshes with a blank item) were refused by `Workbooks.Open` until a numeric cache field with a blank item stopped being
flagged `containsSemiMixedTypes="0"` (Excel needs 1 when text or a blank is present; bisected by editing the saved package).
All 33 then open with no repair log and match Excel's opened values (the layout-swapped pivot after Excel's refresh excepted).
