# Builds crates/calc-doc/tests/fixtures/d1/excel_d1.xlsx in Excel itself (COM), so the package is
# what Excel writes for dynamic arrays, legacy array formulas, spill and calc errors, defined names
# of both scopes, a table with a totals row, cell styles and number formats.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-d1-fixture.ps1 -OutPath <file.xlsx>
#
# Excel's bytes are not reproducible (timestamps, GUIDs), so the committed workbook is one run's
# output; the tests read it as a fixture and never regenerate it. The script starts its own Excel
# instance and quits only that instance.

param(
    [Parameter(Mandatory = $true)][string]$OutPath
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
if (-not [System.IO.Path]::IsPathRooted($OutPath)) { $OutPath = Join-Path (Get-Location).Path $OutPath }
if (Test-Path $OutPath) { Remove-Item $OutPath }

Add-Type -Namespace D1 -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'
$excel = New-Object -ComObject Excel.Application
[uint32]$excelPid = 0
[void][D1.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
try {
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $book = $excel.Workbooks.Add()
    while ($book.Worksheets.Count -lt 2) { [void]$book.Worksheets.Add([System.Reflection.Missing]::Value, $book.Worksheets.Item($book.Worksheets.Count)) }
    $data = $book.Worksheets.Item(1)
    $data.Name = 'Data'
    $calc = $book.Worksheets.Item(2)
    $calc.Name = 'Calc'

    # Data: numbers and texts, and a table with a totals row.
    for ($i = 1; $i -le 5; $i++) {
        $data.Range("A$i").Value2 = [double]$i
        $data.Range("B$i").Value2 = "item$i"
    }
    $data.Range('D1').Value2 = 'Region'
    $data.Range('E1').Value2 = 'Units'
    $data.Range('F1').Value2 = 'Price'
    $regions = @('North', 'South', 'East', 'West', 'Centre')
    for ($i = 0; $i -lt 5; $i++) {
        $row = $i + 2
        $data.Range("D$row").Value2 = [string]$regions[$i]
        $data.Range("E$row").Value2 = [double](($i + 1) * 10)
        $data.Range("F$row").Value2 = [double](1.5 + $i)
    }
    $table = $data.ListObjects.Add(1, $data.Range('D1:F6'), $null, 1)
    $table.Name = 'Sales'
    $table.ShowTotals = $true
    $table.ListColumns.Item(2).TotalsCalculation = 1

    # Styles: bold, a fill, a percentage, a date and a currency format.
    $data.Range('A1').Font.Bold = $true
    $data.Range('A2').Interior.Color = 65535
    $data.Range('A3').NumberFormat = '0.00%'
    $data.Range('A4').NumberFormat = 'yyyy-mm-dd'
    $data.Range('A5').NumberFormat = '#,##0.00 [$EUR]'

    # Names: a workbook-scoped constant, a sheet-scoped range and a hidden name.
    [void]$book.Names.Add('Rate', '=0.05')
    [void]$calc.Names.Add('Local', '=Calc!$A$1:$B$3')
    $hidden = $book.Names.Add('HiddenTotal', '=SUM(Data!$A$1:$A$5)')
    $hidden.Visible = $false

    # Calc: dynamic arrays (spilling, blocked, empty), legacy arrays, and ordinary formulas.
    $calc.Range('A1').Formula2 = '=SEQUENCE(3,2)'
    $calc.Range('D1').Formula2 = '=SORT(Data!A1:A5,,-1)'
    $calc.Range('F1').Formula2 = '=Data!A1:A3*2'
    $calc.Range('H2').Value2 = 'blocker'
    $calc.Range('H1').Formula2 = '=SEQUENCE(2)'
    $calc.Range('J1').Formula2 = '=FILTER(Data!A1:A5,Data!A1:A5>100)'
    $calc.Range('L1:L3').FormulaArray = '=Data!A1:A3*10'
    $calc.Range('N1').FormulaArray = '=SUM(Data!A1:A5*Data!A1:A5)'
    $calc.Range('P1').Formula2 = '=SUM(A1#)'
    $calc.Range('Q1').Formula2 = '=Rate*2'
    $calc.Range('R1').Formula2 = '=SUM(Sales[Units])'
    $calc.Range('S1').Formula2 = '=XLOOKUP(3,Data!A1:A5,Data!B1:B5)'
    $calc.Range('T1').Formula2 = '=IFNA(NA(),"none")'
    $calc.Range('U1').Formula2 = '=Data!A1+1'
    $calc.Range('V1').Formula2 = '=SUM(Local)'
    $calc.Range('W1').Formula2 = '=HiddenTotal'
    $calc.Range('U2').Value2 = 7
    $calc.Range('U3').Formula2 = '=U2*3'

    $excel.Calculate()
    # 51: xlOpenXMLWorkbook (.xlsx)
    $book.SaveAs($OutPath, 51)
    $book.Close($false)
}
finally {
    $excel.Quit()
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
    # Only the instance this script started is stopped, and only if Quit left it running.
    if ($excelPid -ne 0) { Get-Process -Id $excelPid -ErrorAction SilentlyContinue | Stop-Process -Force }
}
Write-Output "wrote $OutPath"
