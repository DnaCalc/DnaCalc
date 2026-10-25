# Compares, in Excel, a package DNA Calc saved after inserting and deleting rows and columns
# (dc-jhn) with the package Excel makes when it opens the original and makes the same edits
# itself: whether the saved package opens with a normal load (no repair), and, on every sheet over
# the used ranges of both, whether each cell's formula (as Excel shows it, A1, en-US) and each
# cell's value after a full calculation are the same. A formula that is the same shows that the
# save moved the cells and adjusted each reference as Excel does; a value that is the same shows
# that the workbook calculates as Excel's edited one does.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File compare-structural.ps1 -Original <a.xlsx> `
#       -Saved <a.saved.xlsx> -Steps 'Sheet|rows|insert|5|1;Sheet|columns|delete|3|1' -Out <json> `
#       [-ExcelOut <a.excel.xlsx>]
#
# With -ExcelOut, Excel's edited workbook is also saved there, for compare-structural-parts.py to
# compare what lies on the cells (merged cells, comments, notes, validations, sizes, formats) with
# the saved package.
#
# Each package is opened in its own Excel (alerts off, links never updated, read-only,
# CorruptLoad 0: a normal load that fails rather than repairs) and calculated in full
# (CalculateFull). The edits are made through Excel's own Insert and Delete, its default
# CopyOrigin (the format of the row above or the column to the left). The script stops only the
# Excel instances it started.

param(
    [Parameter(Mandatory = $true)][string]$Original,
    [Parameter(Mandatory = $true)][string]$Saved,
    [Parameter(Mandatory = $true)][string]$Steps,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$Listed = 40,
    # Where Excel's edited workbook is saved, for compare-structural-parts.py; not saved when empty.
    [string]$ExcelOut = ''
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -Namespace D2C -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

function Start-Excel {
    $excel = New-Object -ComObject Excel.Application
    [uint32]$excelPid = 0
    [void][D2C.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $excel.AskToUpdateLinks = $false
    return @($excel, $excelPid)
}
function Stop-Excel($excel, [uint32]$excelPid) {
    try { $excel.Quit() } catch {}
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    if ($excelPid -ne 0) { Get-Process -Id $excelPid -ErrorAction SilentlyContinue | Stop-Process -Force }
}
# Opens a package as the capture worker does (platform/calc-oracle-excel/worker/capture-worker.ps1,
# Open-Workbook): FileName, UpdateLinks 0, ReadOnly, ..., CorruptLoad 0 (a normal load, no repair),
# called through COM's InvokeMember, since Windows PowerShell's own call of Open with these
# arguments fails ("Unable to get the Open property of the Workbooks class").
function Open-Package($excel, [string]$path) {
    $missing = [System.Type]::Missing
    $arguments = [object[]]@($path, 0, $true, $missing, 'dnacalc-no-password', $missing, $true, $missing, $missing, $false, $false, $missing, $false, $false, 0)
    return , [System.__ComObject].InvokeMember('Open', [System.Reflection.BindingFlags]::InvokeMethod, $null, $excel.Workbooks, $arguments)
}
function Column-Letters([int]$n) { $s = ''; while ($n -gt 0) { $r = ($n - 1) % 26; $s = [char](65 + $r) + $s; $n = [math]::Floor(($n - 1) / 26) }; return $s }

# Each sheet's last used row and column, and its formulas and values over A1 to them, by name.
function Read-Sheets($workbook) {
    $sheets = [ordered]@{}
    foreach ($sheet in $workbook.Worksheets) {
        $used = $sheet.UsedRange
        $rows = $used.Row + $used.Rows.Count - 1
        $columns = $used.Column + $used.Columns.Count - 1
        $range = $sheet.Range($sheet.Cells.Item(1, 1), $sheet.Cells.Item($rows, $columns))
        $sheets[$sheet.Name] = @{ rows = $rows; columns = $columns; formulas = $range.Formula; values = $range.Value2 }
    }
    return $sheets
}
function Cell-At($array, [int]$rows, [int]$columns, [int]$row, [int]$column) {
    if ($row -gt $rows -or $column -gt $columns) { return $null }
    if ($rows -eq 1 -and $columns -eq 1) { return $array }
    return $array[$row, $column]
}
function Shown($value) {
    if ($null -eq $value) { return '' }
    if ($value -is [int]) { return "error $value" }
    if ($value -is [double]) { return $value.ToString('R') }
    return [string]$value
}

$edits = New-Object System.Collections.Generic.List[object]
foreach ($text in $Steps.Split(';')) { if ($text -ne '') { $edits.Add([string[]]$text.Split('|')) } }
$record = [ordered]@{
    excel = $null
    original = [System.IO.Path]::GetFileName($Original)
    saved = [System.IO.Path]::GetFileName($Saved)
    steps = @($edits | ForEach-Object { $_ -join ' ' })
    saved_opened_without_repair = $false
    open_error = $null
}

$excelSide = $null
$started = Start-Excel
try {
    $record.excel = "$($started[0].Version) build $($started[0].Build)"
    $book = Open-Package $started[0] (Resolve-Path $Original).Path
    foreach ($step in $edits) {
        $sheet = $book.Worksheets.Item([string]$step[0])
        $first = [int]$step[3]; $last = $first + [int]$step[4] - 1
        if ($step[1] -eq 'rows') { $range = $sheet.Range("$($first):$($last)") }
        else { $range = $sheet.Range($sheet.Cells.Item(1, $first), $sheet.Cells.Item(1, $last)).EntireColumn }
        if ($step[2] -eq 'insert') { [void]$range.Insert() } else { [void]$range.Delete() }
    }
    $started[0].CalculateFull()
    $excelSide = Read-Sheets $book
    if ($ExcelOut -ne '') { $book.SaveAs($ExcelOut, 51) }
    $book.Close($false)
}
finally { Stop-Excel $started[0] $started[1] }

$ourSide = $null
$started = Start-Excel
try {
    try {
        $book = Open-Package $started[0] (Resolve-Path $Saved).Path
        $record.saved_opened_without_repair = $true
    }
    catch { $record.open_error = $_.Exception.Message }
    if ($record.saved_opened_without_repair) {
        $started[0].CalculateFull()
        $ourSide = Read-Sheets $book
        $book.Close($false)
    }
}
finally { Stop-Excel $started[0] $started[1] }

if ($null -ne $ourSide) {
    $formulaCells = 0; $formulasEqual = 0; $valuesEqual = 0; $inputCells = 0; $inputsEqual = 0
    $differences = @()
    $sheetsMissing = @()
    foreach ($name in $excelSide.Keys) {
        if (-not $ourSide.Contains($name)) { $sheetsMissing += $name; continue }
        $theirs = $excelSide[$name]; $ours = $ourSide[$name]
        $rows = [math]::Max($theirs.rows, $ours.rows); $columns = [math]::Max($theirs.columns, $ours.columns)
        for ($r = 1; $r -le $rows; $r++) {
            for ($c = 1; $c -le $columns; $c++) {
                $tf = [string](Cell-At $theirs.formulas $theirs.rows $theirs.columns $r $c)
                $of = [string](Cell-At $ours.formulas $ours.rows $ours.columns $r $c)
                if ($tf -eq '' -and $of -eq '') { continue }
                $tv = Shown (Cell-At $theirs.values $theirs.rows $theirs.columns $r $c)
                $ov = Shown (Cell-At $ours.values $ours.rows $ours.columns $r $c)
                $isFormula = $tf.StartsWith('=') -or $of.StartsWith('=')
                if ($isFormula) {
                    $formulaCells++
                    if ($tf -ceq $of) { $formulasEqual++ }
                    if ($tv -ceq $ov) { $valuesEqual++ }
                } else {
                    $inputCells++
                    if ($tf -ceq $of) { $inputsEqual++ }
                }
                if (($tf -cne $of -or ($isFormula -and $tv -cne $ov)) -and $differences.Count -lt $Listed) {
                    $differences += [ordered]@{ cell = "$name!$(Column-Letters $c)$r"; excel = $tf; excel_value = $tv; saved = $of; saved_value = $ov }
                }
            }
        }
    }
    $record.sheets_missing = $sheetsMissing
    $record.formula_cells = $formulaCells
    $record.formulas_equal = $formulasEqual
    $record.values_equal = $valuesEqual
    $record.input_cells = $inputCells
    $record.inputs_equal = $inputsEqual
    $record.differences = $differences
}
$record | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 $Out
Write-Output ("formula cells {0}, formulas equal {1}, values equal {2}; input cells {3}, inputs equal {4}" -f $record.formula_cells, $record.formulas_equal, $record.values_equal, $record.input_cells, $record.inputs_equal)
