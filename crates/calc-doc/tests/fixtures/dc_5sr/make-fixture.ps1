# Writes, in Excel, the dc-5sr fixture: one workbook saved twice, as an Excel binary workbook
# (features.xlsb, FileFormat 50) and as a workbook (features.xlsx, FileFormat 51), so the test
# dc_5sr_xlsb can compare calc-doc's model of the .xlsb with its model of the .xlsx Excel saved
# from the same workbook.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-fixture.ps1
#
# What the workbook holds:
# - Data: numbers (an RK integer, a real, a negative, 1E+20, 0.1), text (a shared string, a rich
#   string, one with a line break), a logical, an error from a formula; a sum, a shared formula
#   (C2:C5), a legacy array formula (D1:D2), a dynamic array (E1, SEQUENCE), a future function
#   (XLOOKUP), a defined name (Rate), a reference to a sheet whose name needs quotes, a 3-D sum,
#   text operators, LET, a constant written positionally (1.5E-11); styles (bold, a fill, a
#   border, a percent format, wrapped text), a column width, a row height, a hidden row, merged
#   cells, a comment, a list validation, a conditional format, a hyperlink and frozen panes;
# - Other Sheet: one value the formulas read;
# - Tbl: a table with a calculated column and a totals row.
#
# One Excel instance, which this script starts and stops itself (by the PID of its own window).

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -Namespace D5SF -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$excel = New-Object -ComObject Excel.Application
[uint32]$excelPid = 0
[void][D5SF.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
try {
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $book = $excel.Workbooks.Add()
    while ($book.Worksheets.Count -lt 3) { [void]$book.Worksheets.Add([System.Type]::Missing, $book.Worksheets.Item($book.Worksheets.Count)) }
    $data = $book.Worksheets.Item(1); $data.Name = 'Data'
    $other = $book.Worksheets.Item(2); $other.Name = 'Other Sheet'
    $tbl = $book.Worksheets.Item(3); $tbl.Name = 'Tbl'

    $other.Range('A1').Value2 = 10

    $data.Range('A1').Value2 = 1
    $data.Range('A2').Value2 = 2.5
    $data.Range('A3').Value2 = -3
    $data.Range('A4').Value2 = 1E+20
    $data.Range('A5').Value2 = 0.1
    $data.Range('B1').Value2 = 'alpha'
    $data.Range('B2').Value2 = 'bold and plain'
    $data.Range('B2').Characters(1, 4).Font.Bold = $true
    $data.Range('B3').Value2 = $true
    $data.Range('B4').Formula = '=1/0'
    $data.Range('B5').Value2 = "line one`nline two"

    $book.Names.Add('Rate', '=0.05') | Out-Null
    $data.Names.Add('Local', '=Data!$A$1') | Out-Null

    $data.Range('C1').Formula = '=SUM(A1:A5)'
    $data.Range('C2:C5').Formula = '=A2*2'
    $data.Range('D1:D2').FormulaArray = '=A1:A2*10'
    $data.Range('E1').Formula2 = '=SEQUENCE(3)'
    $data.Range('F1').Formula2 = '=XLOOKUP(2.5,A1:A5,A1:A5)'
    $data.Range('F2').Formula = '=Rate*2'
    $data.Range('F3').Formula = "='Other Sheet'!A1+1"
    $data.Range('F4').Formula = "=SUM(Data:'Other Sheet'!A1)"
    $data.Range('F5').Formula = '=IF(A1>0,"pos","neg")'
    $data.Range('F6').Formula = '="a"&"b"&B1'
    $data.Range('F7').Formula2 = '=LET(x,2,x*3)'
    $data.Range('F8').Formula = '=1.5E-11+Local'
    $data.Range('G1').Formula = '=ROUND(PI(),2)'
    $data.Range('G2').Formula = '=IFERROR(1/0,-1)'
    $data.Range('G3').Formula = '=Tbl!C2'

    $data.Range('A1').Font.Bold = $true
    $data.Range('A2').Interior.Color = 0x00FFFF
    $data.Range('A3').Borders.Item(9).LineStyle = 1
    $data.Range('A5').NumberFormat = '0.00%'
    $data.Range('B5').WrapText = $true
    $data.Columns.Item(1).ColumnWidth = 20
    $data.Rows.Item(3).RowHeight = 30
    $data.Rows.Item(6).Hidden = $true
    $data.Range('H1:I1').Merge()
    [void]$data.Range('A1').AddComment('a comment')
    $data.Range('J1').Validation.Add(3, 1, 1, 'x,y,z')
    $format = $data.Range('A1:A5').FormatConditions.Add(1, 5, '=2')
    $format.Interior.Color = 0x0000FF
    [void]$data.Hyperlinks.Add($data.Range('K1'), 'https://example.com/', '', 'a tip', 'a link')
    $data.Activate()
    $excel.ActiveWindow.SplitRow = 1
    $excel.ActiveWindow.SplitColumn = 1
    $excel.ActiveWindow.FreezePanes = $true

    $tbl.Range('A1').Value2 = 'Item'
    $tbl.Range('B1').Value2 = 'Qty'
    $tbl.Range('A2').Value2 = 'a'; $tbl.Range('B2').Value2 = 3
    $tbl.Range('A3').Value2 = 'b'; $tbl.Range('B3').Value2 = 4
    $list = $tbl.ListObjects.Add(1, $tbl.Range('A1:B3'), $null, 1)
    $list.Name = 'Sales'
    $list.ListColumns.Add() | Out-Null
    $list.ListColumns.Item(3).Name = 'Double'
    $list.ListColumns.Item(3).DataBodyRange.Formula = '=[@Qty]*2'
    $list.ShowTotals = $true
    $list.ListColumns.Item(2).TotalsCalculation = 1

    $book.SaveAs((Join-Path $here 'features.xlsb'), 50)
    $book.SaveAs((Join-Path $here 'features.xlsx'), 51)
    $book.Close($false)
    Write-Output ("Excel {0} build {1}" -f $excel.Version, $excel.Build)
}
finally {
    try { $excel.Quit() } catch {}
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    if ($excelPid -ne 0) { Get-Process -Id $excelPid -ErrorAction SilentlyContinue | Stop-Process -Force }
}
