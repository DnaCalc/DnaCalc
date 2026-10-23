# Measures what Excel shows when it opens packages DNA Calc saved, for dc-35p (D2): whether each
# opens with a normal load (no repair) and, without any calculation call, the text Excel shows and
# the value it holds for the cells named, so a cached value, a spilled value or a newer error the
# save wrote is seen as Excel reads it from the file. `cargo xtask capture` cannot measure this: it
# recalculates (CalculateFullRebuild) before it reads a value, by design.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File measure-saved-open.ps1 -Out <open.json> `
#       -Package '<a.xlsx>;<b.xlsx>' -Cells 'Calc!F1;Calc!F2'
#
# Each package is opened in a fresh Excel (automatic calculation, alerts off, links never updated,
# read-only, CorruptLoad 0: a normal load that fails rather than repairs). A package whose workbook
# part asks for a full calculation on load (fullCalcOnLoad) is also opened as a copy without it
# (`<name>.cached.xlsx`), so what Excel reads from the saved values and value metadata alone is
# seen too. The script stops only the Excel instances it started.

param(
    [Parameter(Mandatory = $true)][string]$Out,
    [Parameter(Mandatory = $true)][string]$Package,
    [Parameter(Mandatory = $true)][string]$Cells
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -Namespace D2O -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

function Start-Excel {
    $excel = New-Object -ComObject Excel.Application
    [uint32]$excelPid = 0
    [void][D2O.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    return @($excel, $excelPid)
}
function Stop-Excel($excel, [uint32]$excelPid) {
    try { $excel.Quit() } catch {}
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    if ($excelPid -ne 0) { Get-Process -Id $excelPid -ErrorAction SilentlyContinue | Stop-Process -Force }
}

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$utf8 = New-Object System.Text.UTF8Encoding($false)

# A copy of the package beside it with fullCalcOnLoad="1" removed from the workbook part; $null
# when the workbook part does not hold it.
function Copy-WithoutFullCalc([string]$Path) {
    $copy = [System.IO.Path]::ChangeExtension($Path, '.cached.xlsx')
    Copy-Item $Path $copy -Force
    $zip = [System.IO.Compression.ZipFile]::Open($copy, 'Update')
    try {
        $entry = $zip.GetEntry('xl/workbook.xml')
        $reader = New-Object System.IO.StreamReader($entry.Open(), $utf8)
        $text = $reader.ReadToEnd(); $reader.Close()
        if (-not $text.Contains(' fullCalcOnLoad="1"')) { return $null }
        $entry.Delete()
        $writer = New-Object System.IO.StreamWriter($zip.CreateEntry('xl/workbook.xml').Open(), $utf8)
        $writer.Write($text.Replace(' fullCalcOnLoad="1"', '')); $writer.Close()
    }
    finally { $zip.Dispose() }
    return $copy
}

$paths = @()
foreach ($path in $Package.Split(';')) {
    $full = (Resolve-Path $path).Path
    $paths += $full
    $cached = Copy-WithoutFullCalc $full
    if ($null -ne $cached) { $paths += $cached }
}

$version = $null
$rows = @()
foreach ($full in $paths) {
    $row = [ordered]@{ package = [System.IO.Path]::GetFileName($full); opened = $false; error = $null; cells = [ordered]@{} }
    $started = Start-Excel
    try {
        if ($null -eq $version) { $version = "$($started[0].Version) build $($started[0].Build)" }
        $missing = [System.Reflection.Missing]::Value
        try {
            [object[]]$arguments = @([string]$full, 0, $true, $missing, $missing, $missing, $true, $missing, $missing, $false, $false, $missing, $false, $false, 0)
            $book = [System.__ComObject].InvokeMember('Open', [System.Reflection.BindingFlags]::InvokeMethod, $null, $started[0].Workbooks, $arguments)
            $row.opened = $true
            foreach ($cell in $Cells.Split(';')) {
                $sheetName, $address = $cell.Split('!')
                try {
                    $range = $book.Worksheets.Item($sheetName).Range($address)
                    $value = $range.Value2
                    $shown = if ($value -is [int]) { "error $value" } else { [string]$value }
                    $row.cells[$cell] = [ordered]@{ text = [string]$range.Text; value2 = $shown; formula = [string]$range.Formula2 }
                }
                catch { $row.cells[$cell] = [ordered]@{ error = $_.Exception.Message } }
            }
            $book.Close($false)
        }
        catch { $row.error = $_.Exception.Message }
    }
    finally { Stop-Excel $started[0] $started[1] }
    $rows += $row
}

[ordered]@{ excel = $version; rows = $rows } | ConvertTo-Json -Depth 6 | Set-Content -Path $Out -Encoding UTF8
