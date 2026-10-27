# Measures how Excel stores each worksheet function in an Excel binary workbook (dc-5sr): the
# formula d1/excel-prefixes.json says Excel accepted for each function (its "entered" text) is put
# in column A of a new workbook through Range.Formula2, row by row, with calculation manual, and
# the workbook saved as .xlsb (FileFormat 50) to <xlsb>. read-function-ids.py then reads, from
# each cell's parsed formula, the token that calls the function: its function number, and whether
# the call names its argument count (PtgFuncVar) or the function has a fixed one (PtgFunc).
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File measure-functions.ps1 -Prefixes <excel-prefixes.json> -Xlsb <functions.xlsb>
#
# It stops only the Excel it started.

param(
    [Parameter(Mandatory = $true)][string]$Prefixes,
    [Parameter(Mandatory = $true)][string]$Xlsb
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -Namespace D5F -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$rows = (Get-Content -Raw $Prefixes | ConvertFrom-Json).rows
$excel = New-Object -ComObject Excel.Application
[uint32]$excelPid = 0
[void][D5F.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
try {
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $book = $excel.Workbooks.Add()
    $excel.Calculation = -4135
    $sheet = $book.Worksheets.Item(1)
    $row = 0
    foreach ($item in $rows) {
        $row++
        $entered = $item[1]
        if ($null -eq $entered) { continue }
        $sheet.Range("A$row").Formula2 = [string]$entered
    }
    $book.SaveAs($Xlsb, 50)
    $book.Close($false)
}
finally {
    $excel.Quit()
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    if ($excelPid -ne 0) { Get-Process -Id $excelPid -ErrorAction SilentlyContinue | Stop-Process -Force }
}
Write-Output "saved $row rows to $Xlsb with Excel"
