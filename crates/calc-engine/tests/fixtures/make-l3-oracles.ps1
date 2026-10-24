# The generator of L3's (dc-m3u) oracle workbook under crates/calc-engine/tests/fixtures/:
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-l3-oracles.ps1 -OutDir <directory>
#
# writes <directory>/l3/oracle.xlsx by driving Excel through COM: it starts an Excel instance of
# its own, enters every formula with Range.Formula (a legacy formula, as a file without
# dynamic-array metadata holds it), saves, and stops only that instance. Unlike the text-written
# fixtures of calc-fml and calc-func, this workbook needs Excel's own authoring: a table
# (ListObject) and names defined relative to a cell. The bytes therefore differ from run to run;
# the committed workbook is the oracle, and `cargo xtask capture` records what Excel calculates
# from it:
#
#   cargo xtask capture --fixture crates/calc-engine/tests/fixtures/l3/oracle.xlsx \
#       --out crates/calc-engine/tests/captures/l3/oracle.capture.json
#
# l3/oracle.xlsx: 3-D references (sheets Ja, Fe, Ma), structured references to the table Sales
# (sheet Tab), names with relative references, calls of unknown names and INDIRECT text naming a
# defined name (sheet Nm), and legacy formulas at the parameters L3 measured (sheet Lg). Two
# areas of L3 have no oracle workbook, since the capture cannot record them: what-if data tables
# (CaptureRecord version 2 has no kind for a dataTable formula, and the capture refuses the
# workbook), and iterative calculation (with a circular reference Excel 16.0 leaves
# Application.CalculationState at xlPending after CalculateFullRebuild, and the capture waits for
# xlDone until it times out). calc-engine's dc_m3u__data_tables and dc_m3u__iteration cases hold
# the values Excel gave for them through COM.

param(
    [Parameter(Mandatory = $true)][string]$OutDir
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
if (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path (Get-Location).Path $OutDir }

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class L3Window {
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
}
'@

function Save-Workbook($Workbook, [string]$Path) {
    $dir = Split-Path $Path -Parent
    if (-not (Test-Path $dir)) { [void](New-Item -ItemType Directory -Path $dir) }
    if (Test-Path $Path) { Remove-Item $Path }
    $Workbook.SaveAs($Path, 51)
}

function Add-Sheets($Workbook, [string[]]$Names) {
    while ($Workbook.Worksheets.Count -lt $Names.Count) {
        [void]$Workbook.Worksheets.Add([Type]::Missing, $Workbook.Worksheets.Item($Workbook.Worksheets.Count))
    }
    for ($i = 1; $i -le $Names.Count; $i++) { $Workbook.Worksheets.Item($i).Name = $Names[$i - 1] }
}

function Set-Formulas($Sheet, [int]$Column, [int]$FirstRow, [string[]]$Formulas) {
    $row = $FirstRow
    foreach ($formula in $Formulas) {
        $Sheet.Cells.Item($row, $Column).Formula = $formula
        $row++
    }
}

$xl = New-Object -ComObject Excel.Application
$xlPid = 0
[void][L3Window]::GetWindowThreadProcessId([IntPtr]$xl.Hwnd, [ref]$xlPid)
try {
    $xl.DisplayAlerts = $false
    $xl.Visible = $false

    # --- l3/oracle.xlsx ------------------------------------------------------------------------
    $wb = $xl.Workbooks.Add()
    $xl.Iteration = $false
    Add-Sheets $wb @('F', 'Ja', 'Fe', 'Ma', 'Tab', 'Nm', 'Lg')
    $wb.Worksheets.Item('Ja').Range('A1').Value2 = 1
    $wb.Worksheets.Item('Fe').Range('A1').Value2 = 2
    $wb.Worksheets.Item('Ma').Range('A1').Value2 = 3
    $wb.Worksheets.Item('Ja').Range('B1').Value2 = 10
    $wb.Worksheets.Item('Fe').Range('B1').Value2 = 20
    $wb.Worksheets.Item('Ma').Range('B1').Value2 = 'x'
    $t = $wb.Worksheets.Item('Tab')
    $t.Range('A1').Value2 = 'Name'; $t.Range('B1').Value2 = 'Qty'; $t.Range('C1').Value2 = 'Unit [Price]'
    $t.Range('A2').Value2 = 'a'; $t.Range('B2').Value2 = 2; $t.Range('C2').Value2 = 5
    $t.Range('A3').Value2 = 'b'; $t.Range('B3').Value2 = 3; $t.Range('C3').Value2 = 7
    $t.Range('A4').Value2 = 'c'; $t.Range('B4').Value2 = 4; $t.Range('C4').Value2 = 11
    $lo = $t.ListObjects.Add(1, $t.Range('A1:C4'), [Type]::Missing, 1)
    $lo.Name = 'Sales'
    $lo.ShowTotals = $true
    $lo.ListColumns.Item(2).TotalsCalculation = 1
    $f = $wb.Worksheets.Item('F')
    Set-Formulas $f 2 1 @(
        '=Ja:Ma!A1', '=Ja:Ma!A1+1', '=SUM(Ja:Ma!A1)', '=SUM(Ja:Ma!A1:B1)', '=SUM(Ma:Ja!A1)', "=SUM('Ja:Ma'!A1)",
        '=AVERAGE(Ja:Ma!A1)', '=COUNT(Ja:Ma!A1:B1)', '=COUNTA(Ja:Ma!A1:B1)', '=MAX(Ja:Ma!A1)', '=MIN(Ja:Ma!A1)', '=PRODUCT(Ja:Ma!A1)',
        '=STDEV(Ja:Ma!A1)', '=VAR.S(Ja:Ma!A1)', '=STDEV.P(Ja:Ma!A1)', '=AVERAGEA(Ja:Ma!A1:B1)', '=MAXA(Ja:Ma!B1)', '=SUMSQ(Ja:Ma!A1)',
        '=MEDIAN(Ja:Ma!A1)', '=LARGE(Ja:Ma!A1,1)', '=RANK(1,Ja:Ma!A1)', '=INDEX(Ja:Ma!A1,1)', '=ROWS(Ja:Ma!A1:B2)', '=AREAS(Ja:Ma!A1)',
        '=ISREF(Ja:Ma!A1)', '=SUMPRODUCT(Ja:Ma!A1)', '=ABS(Ja:Ma!A1)', '=COUNTIF(Ja:Ma!A1,">0")', '=N(Ja:Ma!A1)', '=CONCAT(Ja:Ma!A1)',
        '=AND(Ja:Ma!A1)', '=OR(Ja:Ma!A1)', '=IFERROR(Ja:Ma!A1,"e")', '=ISERROR(Ja:Ma!A1)', '=SHEETS(Ja:Ma!A1)', '=SHEET(Ja:Ma!A1)',
        '=INDIRECT("Ja:Ma!A1")', '=SUM(Ja:Ma!A1,Ja:Fe!B1)', '=SUM(Ja:Ma!A1:A2 Ja:Ma!A1)', '=COUNTBLANK(Ja:Ma!C1)', '=MODE(Ja:Ma!A1)',
        '=TEXTJOIN(",",TRUE,Ja:Ma!A1)', '=MAX(Ja:Ma!A:A)', '=VARP(Ja:Ma!A1)', '=STDEVA(Ja:Ma!A1:B1)'
    )
    Set-Formulas $f 3 1 @(
        '=SUM(Sales[Qty])', '=Sales[[#Headers],[Qty]]', '=Sales[[#Totals],[Qty]]', '=ROWS(Sales[#All])', '=ROWS(Sales[#Data])', '=ROWS(Sales)', '=SUM(Sales)',
        "=SUM(Sales[[Qty]:[Unit '[Price']]])", '=COUNTA(Sales[#Headers])', '=ROWS(Sales[[#Headers],[#Data]])', '=ROWS(Sales[[#Data],[#Totals]])',
        '=Sales[@Qty]', "=SUM(Sales[Unit '[Price']])", '=COLUMNS(Sales[#All])', '=ROWS(Sales[[#All],[Qty]])',
        '=SUM(Sales[#Totals])', '=Sales[[#Totals],[Name]]', '=COUNTA(Sales[[#Headers],[Name]:[Qty]])', '=SUM(sales[qty])', '=SUM(Sales[Qty],Sales[Qty])',
        "=SUM(Sales[Qty]:Sales[Unit '[Price']])", '=ISREF(Sales)', '=COLUMN(Sales[Qty])'
    )
    $t.Range('E2').Formula = '=Sales[@Qty]'
    $t.Range('E3').Formula = '=Sales[@Qty]*10'
    $t.Range('E8').Formula = '=Sales[@Qty]'
    $t.Range('F3').Formula = '=Sales[Qty]'
    $t.Range('G3').Formula = "=Sales[[#This Row],[Unit '[Price']]]"
    # Names: defined relative to Nm!C3 (the active cell), as RC[-1], R[-1]C and RC1.
    $n = $wb.Worksheets.Item('Nm')
    $n.Activate()
    $n.Range('C3').Select()
    [void]$wb.Names.Add('LeftCell', [Type]::Missing, $true, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, '=RC[-1]')
    [void]$wb.Names.Add('AboveCell', [Type]::Missing, $true, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, '=R[-1]C')
    [void]$wb.Names.Add('ColA', [Type]::Missing, $true, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, [Type]::Missing, '=Nm!RC1')
    [void]$wb.Names.Add('Konst', '=5')
    [void]$wb.Names.Add('Abs', '=Nm!$A$1')
    $n.Range('A1').Value2 = 100
    $n.Range('A3').Value2 = 5; $n.Range('B3').Value2 = 7; $n.Range('B2').Value2 = 9; $n.Range('A4').Value2 = 6
    $n.Range('C3').Formula = '=LeftCell'
    $n.Range('C4').Formula = '=AboveCell'
    $n.Range('D4').Formula = '=ColA'
    $n.Range('C6').Formula = '=LeftCell*2'
    $n.Range('E3').Formula = '=INDIRECT("LeftCell")'
    $n.Range('E4').Formula = '=INDIRECT("Konst")'
    $n.Range('E5').Formula = '=INDIRECT("Abs")'
    $n.Range('F1').Formula = '=FOO(1)'
    $n.Range('F2').Formula = '=foo'
    $n.Range('F3').Formula = '=SUM(FOO(1),2)'
    $n.Range('F4').Formula = '=IFERROR(FOO(1),3)'
    $n.Range('G1').Formula = '=Konst(2)'
    $n.Range('G2').Formula = '=Konst*2'
    # Legacy places.
    $l = $wb.Worksheets.Item('Lg')
    $l.Range('A1').Value2 = 1; $l.Range('A2').Value2 = 2; $l.Range('A3').Value2 = 3
    $l.Range('B1').Value2 = 10; $l.Range('B2').Value2 = 20; $l.Range('B3').Value2 = 30
    Set-Formulas $l 4 1 @(
        '=XLOOKUP(2,A1:A3*1,B1:B3)', '=XLOOKUP(2,A1:A3,B1:B3*1)', '=XLOOKUP(A1:A3*1,A1:A3,B1:B3)', '=XMATCH(2,A1:A3*1)',
        '=MEDIAN(A1:A3*1)', '=LARGE(A1:A3*1,1)', '=SMALL(A1:A3*1,2)', '=STDEV(A1:A3*1)', '=CONCAT(A1:A3*1)', '=MMULT(A1:A3*1,1)',
        '=NPV(0.1,A1:A3*1)', '=CORREL(A1:A3*1,B1:B3)', '=PERCENTILE(A1:A3*1,0.5)', '=FREQUENCY(A1:A3*1,2)', '=MODE(A1:A3*1)',
        '=SLOPE(B1:B3,A1:A3*1)', '=INTERCEPT(B1:B3*1,A1:A3)', '=FORECAST(2,B1:B3,A1:A3*1)', '=SUMX2MY2(A1:A3*1,B1:B3)',
        '=COVAR(A1:A3*1,B1:B3)', '=RSQ(A1:A3*1,B1:B3)', '=GEOMEAN(A1:A3*1)', '=VARP(A1:A3*1)', '=TRIMMEAN(A1:A3*1,0)'
    )
    $f.Activate()
    $xl.CalculateFull()
    Save-Workbook $wb (Join-Path $OutDir 'l3\oracle.xlsx')
    $wb.Close($false)

} finally {
    try { $xl.DisplayAlerts = $false; foreach ($open in @($xl.Workbooks)) { $open.Close($false) }; $xl.Quit() } catch {}
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($xl)
    Start-Sleep -Milliseconds 500
    if (Get-Process -Id $xlPid -ErrorAction SilentlyContinue) { Stop-Process -Id $xlPid -Force }
}
Write-Host "wrote $OutDir\l3\oracle.xlsx"
