# Writes, in Excel, the dc-xuw fixture: a workbook with a worksheet and an Excel 4.0 macro sheet,
# saved as a macro-enabled workbook (macro-sheet.xlsm, FileFormat 52) and as an Excel binary
# workbook (macro-sheet.xlsb, FileFormat 50).
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-fixture.ps1
#
# Data (the worksheet): A1 = 2, A2 = A1*3, A3 = Macro1!A2 (a value the macro sheet holds).
# Macro1 (the macro sheet): A1 'Double', A2 = 21 (a constant), A3 =ARGUMENT("x"),
# A4 =RETURN(x*2), A5 =ALERT("hello",2) (a command), A6 =RETURN(); the name Double names A1.
# Nothing runs: the macros are written, never called, and the workbook is not recalculated by
# them.
#
# One Excel instance, which this script starts and stops itself (by the PID of its own window).

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -Namespace DXUW -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$excel = New-Object -ComObject Excel.Application
[uint32]$excelPid = 0
[void][DXUW.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
try {
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $excel.AutomationSecurity = 3
    $book = $excel.Workbooks.Add()
    while ($book.Worksheets.Count -gt 1) { $book.Worksheets.Item($book.Worksheets.Count).Delete() }
    $data = $book.Worksheets.Item(1)
    $data.Name = 'Data'
    $macro = $book.Excel4MacroSheets.Add([System.Type]::Missing, $data)
    $macro.Name = 'Macro1'
    $macro.Range('A1').Value2 = 'Double'
    $macro.Range('A2').Value2 = 21
    $macro.Range('A3').Formula = '=ARGUMENT("x")'
    $macro.Range('A4').Formula = '=RETURN(x*2)'
    $macro.Range('A5').Formula = '=ALERT("hello",2)'
    $macro.Range('A6').Formula = '=RETURN()'
    [void]$book.Names.Add('Double', '=Macro1!$A$1')
    $data.Range('A1').Value2 = 2
    $data.Range('A2').Formula = '=A1*3'
    $data.Range('A3').Formula = '=Macro1!A2'
    $data.Activate()
    $book.SaveAs((Join-Path $here 'macro-sheet.xlsm'), 52)
    $book.SaveAs((Join-Path $here 'macro-sheet.xlsb'), 50)
    $book.Close($false)
    Write-Output ("Excel {0} build {1}" -f $excel.Version, $excel.Build)
}
finally {
    try { $excel.Quit() } catch {}
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    if ($excelPid -ne 0) { Get-Process -Id $excelPid -ErrorAction SilentlyContinue | Stop-Process -Force }
}
