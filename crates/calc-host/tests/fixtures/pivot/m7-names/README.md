# M7: names and saves that M5 took from memory (Excel 16.0.20527, 2026-11-09)

Bead dc-8wp.16. windows-report-5 listed four facts the M5 code carried from memory, not from a capture: Excel's name for a
blank pivot source header, its name for a new table column (`Column<n>`), the spelling of the collapsed pivot's corner
(`#VALUE!` in the UI28 capture, code 2045 in M5), and Excel's saved file for a collapsed pivot or the one-cell stub (DNA
Calc refused that save). They are measured here, in owned hidden Excel sessions on private desktops (guard: 0 windows, 0
visible, 0 foreground hits on the owner's desktop; no dialog answered, none appeared), by
`tools/excel-session/pivot-job.ps1` kind `m7`, and the code follows the captures.

- `names.tsv`: one line per scenario (60): `act`, `refresh`, `refresh_reopened`, `save` (ok, or Excel's error text), then the
  pivots (`name@range@source@field names`) and tables (`name=range=column names`) after stage 0 built, 1 edited (no refresh),
  2 refreshed, 3 saved and reopened, 4 reopened and refreshed. Scenario ids: `hdr-*` a blank, space, number or repeated
  header of a range source (`hdr-table-*`: of a table source); `tbl-col-*` and `tbln-*` a column added to a table (and the
  headers around it, `tbln-*` the second run that fixed the rule); `col-*` a pivot that took one cell for lack of room;
  `stub-*` a source that lost its fields. Start workbook as in m5-structure (`Src` B2:E14, pivot `pvt_m5` at `Out!C4`), but the
  markers are far away (`Out!A1`, `A60`, `Z5`) so that the pivot has room.
- `cells.tsv`: every non-empty cell of `Out` at stage 2 (scenario, stage, sheet, address, kind, value; error cells carry
  Excel's raw code).
- `corners.tsv`: the top-left cell of the pivot of every `col-*` and `stub-*` stage read as Value2, Text, Formula, PivotCell type.
- Not committed: `result.json` per job, the saved workbooks and the replays. R2 `dnacalc-evidence/excel/pivot-m7-20261109/` (114 files,
  9840880 bytes, copy checked, 0 differences; it also holds the OneCalc look check and the WebView2 smoke log of dc-0f1k.20 and .21).

## What Excel does

Blank source header, worksheet range (`hdr-*`; creating the pivot over the blank header gives the same):
- The field is `Field` and the column's position in the range (1 for the first column of the range, whatever its worksheet
  column): `Field4` for E2 of `Src!B2:E14`, `Field3` and `Field4` for D2:E2, `Field1` for B2; an inserted column before the range
  does not change it (`R2C3:R14C6`, still `Field4`). It is not `Column<n>`.
- A header made of one space is the field ` `; a number header is its text (`42`); a header that repeats an earlier name is
  that name with 2 (`Region2`; with three, a table gave `Region2` and `Region3`).
- A pivot whose used field loses its header (Amount, Product) keeps the other fields and drops that one: Amount blank gives a
  pivot with row and column fields and no value field (see the labels-only layout below).

Table (`hdr-table-*`, `tbl-col-*`, `tbln-*`; the pivot over the table follows the table at its next refresh):
- A header cell cleared or set to empty text is renamed by the table: `Column` and the smallest number no other header has
  (`Column1`; D2:E2 cleared gives `Column1` and `Column2`; with `Column5` in the table it is still `Column1`).
- A column added (`ListColumns.Add`, a column inserted inside the table, a value typed right of the table, a new column at the
  start) is named after the column on its left. If that name ends in a number the new name has the next one (`Column5` gives
  `Column6`, `Col9` gives `Col10`, `column5` gives `column6`, `Column10` gives `Column11`, `Column0` gives `Column1`); if that
  name is taken, the left name with 2 appended (`Column52`, twice). If the left name has no number, or the column is first, it is
  `Column` and the smallest free number (`Column1`, `Column2`, `Column3` for three columns added at the start). The rule is
  `calc_doc_model::added_column_name`; the earlier code took the smallest free number for every added column.

The corner of a pivot that took one cell (`col-*`: layout change, refresh, `Location` or a new pivot onto nonblank cells):
- Live, after the refresh, after the file is saved and reopened, and after it is refreshed again, the cell reads Value2
  -2146826243 (code 2045), Text and Formula `#SPILL!`, PivotCell type 0 (`corners.tsv`, 22 reads of the `col-*` scenarios). The UI28 capture's `#VALUE!` was
  read from the saved file: Excel stores the corner as `<c r="C4" s="1" t="e" vm="1"><v>#VALUE!</v></c>` with the value metadata
  of an `_error` rich value (`errorType` 8 = #SPILL!, `subType` 7, the offsets of the extent the pivot needed).
- The saved file for it is the ordinary pivot definition with `<location ref="C4" firstHeaderRow="0" firstDataRow="0"
  firstDataCol="0"/>` and the fields, `rowItems`, `colItems` and `dataFields` of the whole layout kept. A refresh after the
  blocker is cleared gives the pivot its output again (`col-collapsed-then-room`: C4:D20).

A source that lost its fields (`stub-*`):
- When no field of the pivot has its column any more (the source cleared, all its columns deleted), the pivot is an empty
  pivot, the frame C4:E21 (3 columns by 18 rows from its first cell) of empty cells, with `<location ref="C4:E21"
  firstHeaderRow="1" firstDataRow="1" firstDataCol="0"/>`, one `<pivotField showAll="0"/>` per source column and no
  `rowFields`, `rowItems`, `colFields`, `colItems` or `dataFields`; the cache keeps the 12 records of blanks and the fields
  `Field1`..`Field4`. It is the frame meeting the markers of the M5 scenarios (`src-clear-all`, `src-del-cols-all`) that gave the
  one-cell error stub there: the same collapse as above. M5 took the one-cell stub for the general result; it is not.
- When only the value fields lost their column (Amount deleted, renamed, or blanked; a table column deleted) Excel keeps the row
  and column fields and draws their labels with no values (`stub-delete-value-column`: C4:H9, C4 empty). That layout is not modelled:
  the refresh is refused with its reason.

## What DNA Calc does now (tests in `crates/calc-host/tests/pivot_m5.rs`, `m7` ones last; 17 of 17)

- A blank range header is `Field<n>`; numbers and repeated names are as above; `calc_doc_model::added_column_name`,
  `free_column_name` and `numbered_duplicate` name table columns (inserted columns, blank headers of a new table).
- A collapsed pivot saves as Excel saves it, loads back collapsed (a one-cell location with fields), and shows #SPILL!; the cell is
  written `#VALUE!` with the value metadata. The empty pivot frame saves and loads; a source that loses every field gives the frame,
  or the one-cell collapse when cells are in its way. Excel opened the saves without repair (`replay`, below).
- Not done: the labels-only layout; creating a table over blank headers (the cleared-header rule is applied, not captured);
  editing a table header cell to blank (DNA Calc refuses a header edit that way).

## Replay of DNA Calc's saves

`pivot-job.ps1` kind `open` on the saves of `dc_8wp_16__a_collapsed_pivot_expands_when_room_is_made_and_saves_as_excel_saves_it`
and `dc_8wp_16__a_source_that_loses_every_field_gives_the_empty_pivot_frame_unless_cells_are_in_the_way` (set
`DNACALC_PIVOT_SAVES`): all three open with no repair log, the pivots read `$C$4` and `$C$4:$E$21`, refresh leaves them as they
are, and the collapsed corner reads code 2045. The first replay found the frame refused by `Workbooks.Open`: the save
held `<rowItems count="0">`, `<colItems count="0">` and `<dataFields count="0">` (Excel's own file has none of the three; calc-doc no
longer writes an empty container) and a cache with no records (the refresh now reads the source, so the cleared rows are records of
blanks as in Excel's file). The second replay opened all three.
