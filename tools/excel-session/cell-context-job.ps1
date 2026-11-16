# One owned-Excel job (dc-483v): Excel's answers to a cell-context case set (format dnacalc-cell-context/1, see
# tools/xtask-harness/src/cell_context.rs). Parameters (named, as submit.ps1 passes them):
#   Cases:  a case set (tools/xtask-harness/cell-context/<family>.json);
#   Result: the answered set this writes (the same cases, each with "answer", and capture_provenance).
# Each case is built in a new workbook: its sheets named and ordered as listed (a new workbook starts with one
# worksheet), constants written as Value2 (text into cells formatted as text, errors as error formulas), formulas by
# Formula2, workbook names added, the listed rows and columns hidden, then the case's formula entered in its cell by
# Formula2 (a refused entry is answered "entry:refused"), a full calculation, and the cell read: its Value2 in the
# witness form, and when the formula spills over more than one cell, every cell of the spill range. A case whose
# constants do not read back as written is answered "unread:input-changed" without entering the formula. Each
# workbook is closed unsaved. No setting of Excel is changed.
param($OwnedExcel, [string]$Cases, [string]$Result)
$ErrorActionPreference = 'Stop'
# PowerShell's COM binder caches the Value2 setter by member name: after a double was written, a later string
# write is cast to double (InvalidCastException, seen 2026-11-16 on the first text cell after a number).
# Writing through InvokeMember skips that cache.
function SetValue2($cell, $value) { [void]$cell.GetType().InvokeMember('Value2', [Reflection.BindingFlags]::SetProperty, $null, $cell, @($value)) }
$codes = @{2000='Null';2007='Div0';2015='Value';2023='Ref';2029='Name';2036='Num';2042='NA';2043='GettingData';2045='Spill';2046='Connect';2047='Blocked';2048='Unknown';2049='Field';2050='Calc'}
$literals = @{Null='#NULL!';Div0='#DIV/0!';Value='#VALUE!';Ref='#REF!';Name='#NAME?';Num='#NUM!';NA='#N/A'}
function Form($v) {
    if ($null -eq $v) { return 'blank' }
    if ($v -is [double]) { return '0x' + ('{0:x16}' -f [BitConverter]::DoubleToInt64Bits($v)) }
    if ($v -is [string]) { return 'text:' + $v }
    if ($v -is [bool]) { return 'logical:' + $(if ($v) { 'True' } else { 'False' }) }
    if ($v -is [int]) { $n = $v - (-2146826288) + 2000; if ($codes.ContainsKey($n)) { return 'error:' + $codes[$n] } ; return 'error:code' + $v }
    return 'unread:' + $v.GetType().Name
}
function Cell($ws, [string]$a1) { $ws.Range($a1) }
function Place($book, [string]$at) {
    $i = $at.LastIndexOf('!')
    $sheet = $at.Substring(0, $i).Trim("'").Replace("''", "'")
    return @($book.Worksheets.Item($sheet), $at.Substring($i + 1))
}
$set = Get-Content -LiteralPath $Cases -Raw -Encoding utf8 | ConvertFrom-Json
# Excel's own clock before and after the cases (two-part dates such as 15/3 take its year).
$clockBefore = [double]$OwnedExcel.Evaluate('NOW()')
$answered = New-Object System.Collections.Generic.List[object]
$progress = $Result + '.progress'
[IO.File]::WriteAllText($progress, '')
foreach ($case in $set.cases) {
    # One line per case before it runs: a job that stops shows the case it stopped in.
    [IO.File]::AppendAllText($progress, [string]$case.id + "`n")
    # xlWBATWorksheet: a new workbook of exactly one worksheet.
    $book = $OwnedExcel.Workbooks.Add(-4167)
    try {
        # A case names a layout of the set (its sheets and names).
        $layout = if ($case.PSObject.Properties['layout']) { $set.layouts.([string]$case.layout) } else { $case }
        $sheets = @($layout.sheets)
        $first = $book.Worksheets.Item(1)
        $first.Name = [string]$sheets[0].name
        $previous = $first
        for ($s = 1; $s -lt $sheets.Count; $s++) {
            $ws = $book.Worksheets.Add([Type]::Missing, $previous)
            $ws.Name = [string]$sheets[$s].name
            $previous = $ws
        }
        $constants = New-Object System.Collections.Generic.List[object]
        foreach ($sheet in $sheets) {
            $ws = $book.Worksheets.Item([string]$sheet.name)
            $formulas = @()
            foreach ($p in $sheet.cells.PSObject.Properties) {
                if (-not ([string]$p.Value).StartsWith('=')) { $constants.Add(@($ws.Range($p.Name), [string]$p.Value)) }
                $v = [string]$p.Value
                $cell = $ws.Range($p.Name)
                if ($v.StartsWith('=')) { $formulas += ,@($cell, $v) }
                elseif ($v.StartsWith('0x')) { SetValue2 $cell ([BitConverter]::Int64BitsToDouble([Convert]::ToInt64($v.Substring(2), 16))) }
                elseif ($v.StartsWith('text:')) { $cell.NumberFormat = '@'; SetValue2 $cell ($v.Substring(5)) }
                elseif ($v -ceq 'logical:true' -or $v -ceq 'logical:True') { SetValue2 $cell ($true) }
                elseif ($v -ceq 'logical:false' -or $v -ceq 'logical:False') { SetValue2 $cell ($false) }
                elseif ($v.StartsWith('error:')) { $cell.Formula = '=' + $literals[$v.Substring(6)] }
                else { throw "case $($case.id): cell $($p.Name) holds $v" }
            }
            foreach ($f in $formulas) { $f[0].Formula2 = $f[1] }
        }
        if ($layout.PSObject.Properties['names'] -and $layout.names) {
            foreach ($p in $layout.names.PSObject.Properties) { [void]$book.Names.Add($p.Name, [string]$p.Value) }
        }
        foreach ($sheet in $sheets) {
            $ws = $book.Worksheets.Item([string]$sheet.name)
            foreach ($r in @($sheet.hidden_rows)) { if ($null -ne $r) { $ws.Rows.Item([int]$r).Hidden = $true } }
            foreach ($c in @($sheet.hidden_columns)) { if ($null -ne $c) { $ws.Columns.Item([int]$c).Hidden = $true } }
        }
        $place = Place $book ([string]$case.at)
        $target = $place[0].Range($place[1])
        $answer = [ordered]@{value = $null; spill = $null}
        $refused = $false
        # Every constant must read back as written (a leading apostrophe, say, would become a prefix).
        $changed = @($constants | Where-Object {
            $back = Form $_[0].Value2
            $want = $_[1] -replace '^logical:true$', 'logical:True' -replace '^logical:false$', 'logical:False'
            $back -cne $want })
        if ($changed.Count -gt 0) { $refused = $true; $answer.value = 'unread:input-changed' }
        if (-not $refused) {
            try { $target.Formula2 = [string]$case.formula } catch { $refused = $true; $answer.value = 'entry:refused'; $answer.entry_error = $_.Exception.Message }
        }
        if (-not $refused) {
            $OwnedExcel.CalculateFull()
            $answer.value = Form $target.Value2
            if ([bool]$target.HasSpill) {
                $range = $target.SpillingToRange
                if ([int]$range.Count -gt 1) {
                    $values = $range.Value2
                    $rows = @()
                    for ($r = 1; $r -le [int]$range.Rows.Count; $r++) {
                        $rows += ,@(for ($c = 1; $c -le [int]$range.Columns.Count; $c++) { Form $values[$r, $c] })
                    }
                    $answer.spill = $rows
                }
            }
        }
        $copy = [ordered]@{}
        foreach ($p in $case.PSObject.Properties) { $copy[$p.Name] = $p.Value }
        $copy.answer = $answer
        $answered.Add($copy)
    } finally {
        $book.Close($false)
    }
}
$clockAfter = [double]$OwnedExcel.Evaluate('NOW()')
$out = [ordered]@{
    format = 'dnacalc-cell-context/1'; family = $set.family; layouts = $set.layouts; cases = $answered.ToArray()
    capture_provenance = [ordered]@{
        environment = [ordered]@{excel_build = [string]$OwnedExcel.Build; excel_version = [string]$OwnedExcel.Version; platform = 'Windows'; os = [Environment]::OSVersion.VersionString; cpu = $env:PROCESSOR_IDENTIFIER}
        calculation_settings = [ordered]@{user_culture = [Globalization.CultureInfo]::CurrentCulture.Name; international_decimal_separator = [string]$OwnedExcel.International(3); international_thousands_separator = [string]$OwnedExcel.International(4); UseSystemSeparators = [bool]$OwnedExcel.UseSystemSeparators; calculation_clock = ('{0:x16}' -f [BitConverter]::DoubleToInt64Bits($clockBefore)); calculation_clock_after = ('{0:x16}' -f [BitConverter]::DoubleToInt64Bits($clockAfter))}
        addins_open = @(foreach ($a in $OwnedExcel.AddIns2) { if ([bool]$a.IsOpen) { [IO.Path]::GetFileName([string]$a.FullName) } })
        service = 'tools/excel-session/cell-context-job.ps1'; timestamp = [DateTime]::UtcNow.ToString('o')}
}
[IO.File]::WriteAllText($Result, (ConvertTo-Json -InputObject $out -Depth 12) + "`n", [Text.UTF8Encoding]::new($false))
