# Excel AutoFit measurements

The 512 readings in excel-20625-96dpi.csv were captured on 2026-11-25 using
64-bit Excel 16.0.20625.20108 on an AMD Ryzen 7 4800H. Excel ran on the ww19
private desktop at 100% workbook zoom. GetDpiForWindow reported 96 DPI.
Calibri 11 and Arial 10 were used as the workbook's Normal style.

Each row records the requested operation and input, Excel's displayed text,
the Normal and cell fonts, and the Excel column width and row height. The
column_points, row_height and standard_height fields are in points.
The input and text fields escape line breaks as \n. The row cases cover
explicit breaks, wrapping at three widths, indentation, rotation,
shrink-to-fit, merged cells and font sizes 8 through 28.

Raw measurements with exact binary64 values are in the Windows lane scratch
directory at ww19/autofit3/observations.jsonl. The 120 DPI repeat and the
CalcHost AutoFit and WrapText implementation remain under dc-igr0p.
