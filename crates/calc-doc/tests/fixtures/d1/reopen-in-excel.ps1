# Opens each .xlsx of a directory in a fresh Excel instance and records whether Excel reports a
# repair, for dc-ndi (D1): the saves its tests write when DNACALC_D1_SAVE_DIR is set.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File reopen-in-excel.ps1 -Dir <directory> -Out <result.json>
#
# For each workbook the script starts its own Excel (one instance per file), opens the file with
# CorruptLoad 0 (a normal load) and alerts off, and records:
#   - opened: whether Workbooks.Open returned a workbook;
#   - repaired: whether Excel reported a repair, by any of three signs: the normal load failed
#     (with alerts off, Excel 16 refuses a normal load of a package it would repair, as the control
#     shows), the workbook's or window's caption holds "Repaired", or Excel wrote a repair log
#     (error*.xml) to the temp directory while the file was open;
#   - values: the displayed text of the cells named by -Cells (sheet!address), after a full
#     recalculation.
# It also builds a positive control first (-Control): a copy of the first workbook whose first
# worksheet gains a cell with a style index past the styles part's cell formats, which Excel must
# repair; the run fails unless the control is reported repaired.
# The script stops only the Excel processes it started.

param(
    [Parameter(Mandatory = $true)][string]$Dir,
    [Parameter(Mandatory = $true)][string]$Out,
    # Cells to read after a full recalculation, comma-separated: Sheet!A1,Sheet!B2.
    [string]$Cells = '',
    [switch]$Control
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -Namespace D1R -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$Dir = (Resolve-Path $Dir).Path
$files = @(Get-ChildItem -Path $Dir -Filter '*.xlsx' | Where-Object { $_.Name -notlike '~$*' } | Sort-Object Name)
if ($files.Count -eq 0) { throw "no .xlsx under $Dir" }

function New-CorruptControl([string]$Source, [string]$Target) {
    Copy-Item $Source $Target -Force
    $zip = [System.IO.Compression.ZipFile]::Open($Target, 'Update')
    try {
        $entry = $zip.Entries | Where-Object { $_.FullName -like 'xl/worksheets/sheet*.xml' } | Sort-Object FullName | Select-Object -First 1
        $reader = New-Object System.IO.StreamReader($entry.Open())
        $xml = $reader.ReadToEnd()
        $reader.Close()
        $xml = $xml.Replace('<sheetData>', '<sheetData><row r="1000"><c r="A1000" s="9999"><v>1</v></c></row>')
        $name = $entry.FullName
        $entry.Delete()
        $new = $zip.CreateEntry($name)
        $writer = New-Object System.IO.StreamWriter($new.Open(), (New-Object System.Text.UTF8Encoding($false)))
        $writer.Write($xml)
        $writer.Close()
    }
    finally { $zip.Dispose() }
}

function Test-Reopen([string]$Path) {
    $logsBefore = @(Get-ChildItem -Path $env:TEMP -Filter 'error*.xml' -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
    $excel = New-Object -ComObject Excel.Application
    [uint32]$excelPid = 0
    [void][D1R.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
    $result = [ordered]@{ file = (Split-Path $Path -Leaf); opened = $false; repaired = $false; signs = @(); values = [ordered]@{}; error = $null }
    try {
        $excel.Visible = $false
        $excel.DisplayAlerts = $false
        $missing = [System.Reflection.Missing]::Value
        $book = $null
        try {
            # FileName, UpdateLinks 0, ReadOnly, Format, Password, WriteResPassword,
            # IgnoreReadOnlyRecommended, Origin, Delimiter, Editable, Notify, Converter, AddToMru,
            # Local, CorruptLoad 0.
            # Called through COM's own call (InvokeMember): Windows PowerShell's call of Open with
            # these arguments fails ("Unable to get the Open property of the Workbooks class"), as
            # the capture worker records.
            [object[]]$arguments = @([string]$Path, 0, $true, $missing, $missing, $missing, $true, $missing, $missing, $false, $false, $missing, $false, $false, 0)
            $book = [System.__ComObject].InvokeMember('Open', [System.Reflection.BindingFlags]::InvokeMethod, $null, $excel.Workbooks, $arguments)
        }
        catch { $result.error = $_.Exception.Message }
        if ($null -ne $book) {
            $result.opened = $true
            $captions = @([string]$book.Name)
            try { $captions += [string]$excel.ActiveWindow.Caption } catch { }
            foreach ($caption in $captions) {
                if ($caption -match 'Repaired') { $result.signs += "caption: $caption" }
            }
            $excel.CalculateFull()
            foreach ($cell in @($Cells.Split(',') | Where-Object { $_ -ne '' })) {
                $sheetName, $address = $cell.Split('!')
                try {
                    $value = $book.Worksheets.Item($sheetName).Range($address).Text
                    $result.values[$cell] = [string]$value
                }
                catch { $result.values[$cell] = "<not read: $($_.Exception.Message)>" }
            }
            $book.Close($false)
        }
    }
    finally {
        $excel.Quit()
        [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
        [GC]::Collect()
        [GC]::WaitForPendingFinalizers()
        if ($excelPid -ne 0) { Get-Process -Id $excelPid -ErrorAction SilentlyContinue | Stop-Process -Force }
    }
    $logsAfter = @(Get-ChildItem -Path $env:TEMP -Filter 'error*.xml' -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
    foreach ($log in $logsAfter) {
        if ($logsBefore -notcontains $log) { $result.signs += "repair log: $log" }
    }
    # A normal load (CorruptLoad 0) with alerts off fails outright for a package Excel would have
    # to repair (measured by the control below), so a failed open counts as a repair too.
    if (-not $result.opened) { $result.signs += "a normal load failed: $($result.error)" }
    $result.repaired = ($result.signs.Count -gt 0)
    return $result
}

$results = @()
if ($Control) {
    $controlDir = Join-Path ([System.IO.Path]::GetTempPath()) ('d1-control-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $controlDir | Out-Null
    $controlPath = Join-Path $controlDir 'control_corrupt.xlsx'
    New-CorruptControl $files[0].FullName $controlPath
    $controlResult = Test-Reopen $controlPath
    $controlResult.file = 'control_corrupt.xlsx (positive control: style index past the cell formats)'
    $results += $controlResult
    Remove-Item $controlDir -Recurse -Force
    if (-not $controlResult.repaired) { throw "the positive control was not reported repaired: $($controlResult | ConvertTo-Json)" }
}
foreach ($file in $files) { $results += Test-Reopen $file.FullName }

$excelVersion = $null
$probe = New-Object -ComObject Excel.Application
[uint32]$probePid = 0
[void][D1R.Win]::GetWindowThreadProcessId([System.IntPtr]$probe.Hwnd, [ref]$probePid)
try { $excelVersion = "$($probe.Version) build $($probe.Build)" } finally {
    $probe.Quit(); [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($probe)
    if ($probePid -ne 0) { Get-Process -Id $probePid -ErrorAction SilentlyContinue | Stop-Process -Force }
}
$report = [ordered]@{ excel = $excelVersion; results = $results }
$report | ConvertTo-Json -Depth 5 | Set-Content -Path $Out -Encoding UTF8
$report | ConvertTo-Json -Depth 5
