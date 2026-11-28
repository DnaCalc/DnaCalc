# Macro-built report fixture (dc-piw0.4)

Build the same report in Excel and DNA Calc, then compare their saved workbooks.

- `report-data.tsv`: the data grid (header row, five regions, Q1/Q2 numbers, a `Total` formula column).
- `BuildReport.bas`: `BuildReport.Build` fills the active sheet and adds the table `Sales`,
  the column chart `SalesChart` (columns A:C, title "Sales by region", legend) and the line chart
  `SalesTrend` (columns A:B, title "Q1 sales"). It adds five drawing objects (`Start`, `Finish`, the connector `Link`
  glued Start site 3 to Finish site 1, the text box `Note`, the line `Rule`). Only descriptor-registered members.
- `DumpReport.bas`: `DumpReport.Dump` lists cells, tables, charts, series and shapes as tab-separated records (format in
  its header) to a sheet `Listing` of a new workbook and to `ListingPath`.
- `cargo test -p calc-host-vba --test calc-host-vba-integration dc_piw0_4_build_report_macro -- --nocapture`
  runs the authored VBA source through the workbook command handler, saves and reopens the result.
  Set `DNACALC_REPORT_OUTPUT_DIR` to save the authored input and the Interpreter and JIT outputs
  (`report-source.xlsm`, `report-interpreter.xlsm` and `report-jit.xlsm`).
- `cargo test -p calc-host --test calc-host-integration automation::dc_piw0_4 -- --nocapture`
  prints the listing from the object-model control. Set `DNACALC_REPORT_LISTING` to write its 42 records.

In Excel, open a new workbook and import `BuildReport.bas` and `DumpReport.bas`. Run `Build`, save
the report and run `Dump`. Open each saved DNA Calc output in Excel and use the same listing macro
to compare the results. Keep the saved packages for the chart and drawing XML comparison.
