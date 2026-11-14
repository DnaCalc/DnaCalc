# Macro-built report fixture (dc-piw0.4)

One macro builds a report; Excel's run is the answer, DNA Calc's run is compared with it.

- `report-data.tsv`: the data grid (header row, five regions, Q1/Q2 numbers, a `Total` formula column).
- `BuildReport.bas`: `BuildReport.Build` fills the grid on the active sheet, adds the table `Sales`, the chart `SalesChart`
  (columns A:C, title "Sales by region", legend) and five drawing objects (`Start`, `Finish`, the connector `Link`
  glued Start site 3 to Finish site 1, the text box `Note`, the line `Rule`). Only descriptor-registered members.
- `DumpReport.bas`: `DumpReport.Dump` lists cells, tables, charts, series and shapes as tab-separated records (format in
  its header) to a sheet `Listing` of a new workbook and to `ListingPath`.
- DNA Calc side: `cargo test -p calc-host --test automation dc_piw0_4 -- --nocapture` runs the same steps through the
  object-model handlers and prints the same records; `DNACALC_REPORT_LISTING=<file>` also writes them to a file.
  Diff that file with Excel's `report-listing.txt`.

Excel run: open a new workbook, import `BuildReport.bas` and `DumpReport.bas`, run `Build`, then `Dump`; also
`ActiveWorkbook.SaveCopyAs` the report for the part-by-part comparison. `FORMULA` records appear in both listings;
in DNA Calc `Range.Formula` (get and put) is a handler (dc-piw0.6), so the macro's formula column goes through it.
