# The witness worker of calc-oracle-excel (bead dc-ymjf.3): reads Excel's result of one worksheet
# function on many argument rows, in bulk, through Range.Value2. The adapter (src/witness.rs)
# writes this script, and the capture worker whose functions it uses, into a fresh work directory
# and starts one Windows PowerShell process per request with a request file; it is not run by hand.
#
# It runs under the capture worker's isolation and reuses its functions (it dot-sources
# capture-worker.ps1, which then defines its functions and starts nothing): the new Excel instance
# whose PID it prints before anything else touches it, the session settings (DisplayAlerts,
# ScreenUpdating, EnableEvents and Interactive off), each read back, and the
# add-in checks. It opens no file: it adds one empty workbook. It writes no option of the user's
# profile. AutomationSecurity is only read, preserved and recorded; no file is opened.
#
# What it does, in order:
#   1. starts Excel as above and adds an empty workbook, whose calculation it sets to manual;
#   2. for each chunk of at most 50,000 rows of the request: writes the arguments as typed values
#      through one Range.Value2 block per argument column (columns B onwards; a number as the
#      double of its bits, a logical as a boolean, text into a cell formatted as text "@"), never
#      as formula text, so Excel parses no literal (OxFunc's EXCEL_RUNNER_PLUMBING_NOTE: literal
#      parsing moved about a third of a run's rows);
#   3. writes each row's formula into column A through Formula2R1C1, one assignment per run of rows
#      with the same formula (the adapter writes it, for example =GAMMALN(RC[1]) or
#      =INDEX(MINVERSE(RC[1]),1,2)), then calls Calculate and waits for CalculationState xlDone;
#   4. reads column A's Value2 in one block (each value through Convert-Value: a number as its
#      binary64 bits, text as UTF-16 units, a logical, or Excel's CVErr code) and the argument
#      columns' Value2, and marks a row whose arguments do not read back as written (input_kept
#      false: Excel changed an input, so that reading is not of the inputs asked for);
#   5. clears the sheet; after the last chunk records Excel's version, build, file version, the
#      bitness of EXCEL.EXE, the OS and the CPU, quits Excel, writes the result file and prints
#      "done".
#
# The script is ASCII and runs under Windows PowerShell 5.1.

param(
    [string]$Request,
    [string]$Result,
    $OwnedExcel = $null
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

# Dot-sourcing runs the capture worker's param block in this scope, which binds its -Request and
# -Result to nothing, so this script's own are kept apart first.
$WitnessRequest = $Request
$WitnessResult = $Result
$WitnessOwnedExcel = $OwnedExcel
. (Join-Path $PSScriptRoot 'capture-worker.ps1')
$Request = $WitnessRequest
$Result = $WitnessResult
$OwnedExcel = $WitnessOwnedExcel

if ([string]::IsNullOrEmpty($Request) -or [string]::IsNullOrEmpty($Result)) {
    [Console]::Error.WriteLine('witness-worker.ps1 needs -Request <request.json> and -Result <result.json>')
    exit 2
}

$Output = [ordered]@{
    format = 'dnacalc-witness-worker-result'
    version = 1
    excel_pid = $null
    fatal = $null
    excel_version = $null
    excel_build = $null
    excel_file_version = $null
    excel_machine = $null
    os_version = $null
    cpu = $null
    automation_security_before = $null
    automation_security_after = $null
    calculation_settings = $null
    rows = (New-Object System.Collections.ArrayList)
    failures = $Failures
}

$ChunkRows = 50000

# The typed value a request argument names: a double from its bits, a boolean, or a string.
function Get-ArgumentValue($Argument) {
    if ($null -ne $Argument.PSObject.Properties['number']) {
        return [BitConverter]::Int64BitsToDouble([Convert]::ToInt64([string]$Argument.number, 16))
    }
    if ($null -ne $Argument.PSObject.Properties['logical']) { return [bool]$Argument.logical }
    if ($null -ne $Argument.PSObject.Properties['text']) { return [string]$Argument.text }
    throw ('an argument is neither a number, a logical nor text: ' + ($Argument | ConvertTo-Json -Compress))
}

# Calls $Action, then waits for Application.CalculationState to read xlDone (0).
function Wait-Calculation($Application, [scriptblock]$Action) {
    & $Action
    while ($true) {
        $state = Get-ComProperty $Application 'CalculationState'
        if (-not ($state -is [int] -or $state -is [long])) { throw ('CalculationState reads as ' + (Get-Shown $state)) }
        if ([long]$state -eq 0) { break }
        Start-Sleep -Milliseconds 1
    }
}

# The Value2 of a range as a list of values in row order (a one-cell range reads as one value).
function Read-Column($Range, [int]$Count) {
    $values = Get-ComProperty $Range 'Value2'
    $list = New-Object System.Collections.ArrayList
    if ($Count -eq 1) { [void]$list.Add($values); return , $list }
    for ($i = 1; $i -le $Count; $i++) { [void]$list.Add($values.GetValue($i, 1)) }
    return , $list
}

# Whether a value read back is the value written: the same bits for a number, the same boolean,
# the same string.
function Test-Kept($Written, $Read) {
    if ($Written -is [double]) {
        return ($Read -is [double]) -and ([BitConverter]::DoubleToInt64Bits($Read) -eq [BitConverter]::DoubleToInt64Bits($Written))
    }
    if ($Written -is [bool]) { return ($Read -is [bool]) -and ($Read -eq $Written) }
    return ($Read -is [string]) -and ([string]::CompareOrdinal($Read, $Written) -eq 0)
}

$excel = $null
$workbook = $null
try {
    $req = [System.IO.File]::ReadAllText($Request, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
    $rows = @($req.rows)

    if ($null -ne $OwnedExcel) { $excel = $OwnedExcel }
    else { $excel = New-Object -ComObject Excel.Application }
    $excelPid = Get-ExcelPid $excel
    $Output.excel_pid = $excelPid
    Send-Line "excel-pid $excelPid"

    $securityBefore = Get-ComProperty $excel 'AutomationSecurity'
    if ($securityBefore -notin @(1, 2, 3)) { throw 'AutomationSecurity could not be read as a known value' }
    $Output.automation_security_before = [long]$securityBefore
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $excel.ScreenUpdating = $false
    $excel.EnableEvents = $false
    $excel.Interactive = $false
    $expected = [ordered]@{ DisplayAlerts = $false; EnableEvents = $false; Interactive = $false; AutomationSecurity = $securityBefore }
    $notIsolated = New-Object System.Collections.ArrayList
    foreach ($name in $expected.Keys) {
        $want = $expected[$name]
        $value = Get-ComProperty $excel $name
        if ($want -is [bool]) {
            $same = ($value -is [bool]) -and ($value -eq $want)
        } else {
            $same = ($value -is [int] -or $value -is [long] -or $value -is [int16]) -and ([long]$value -eq [long]$want)
        }
        if (-not $same) { [void]$notIsolated.Add("$name reads back as " + (Get-Shown $value) + " after it was set to $want") }
    }
    $addIns = Get-AddInStates $excel
    foreach ($entry in $addIns.loaded) { [void]$notIsolated.Add($entry) }
    if ($notIsolated.Count -gt 0) {
        throw ('refused before any formula was written: this Excel instance is not isolated: ' + ($notIsolated -join '; '))
    }

    $workbook = Invoke-ComMethod $excel.Workbooks 'Add' @()
    $excel.Calculation = -4135
    $calculation = Get-ComProperty $excel 'Calculation'
    if ([long]$calculation -ne -4135) { throw ('Calculation reads as ' + (Get-Shown $calculation) + ' after it was set to manual (-4135)') }
    $sheet = $workbook.Worksheets.Item(1)
    $settings = [ordered]@{ calculation = [long]$calculation; compatibility_version = $null; unavailable = @() }
    foreach ($name in @('PrecisionAsDisplayed', 'Date1904', 'CompatibilityVersion')) {
        try { $settings[$name] = Get-ComProperty $workbook $name }
        catch { $settings.unavailable += $name }
    }
    foreach ($name in @('UseSystemSeparators', 'DecimalSeparator', 'ThousandsSeparator')) {
        try { $settings[$name] = Get-ComProperty $excel $name }
        catch { $settings.unavailable += $name }
    }
    $Output.calculation_settings = $settings

    for ($start = 0; $start -lt $rows.Count; $start += $ChunkRows) {
        $count = [Math]::Min($ChunkRows, $rows.Count - $start)
        $width = 0
        for ($i = 0; $i -lt $count; $i++) { $width = [Math]::Max($width, @($rows[$start + $i].args).Count) }
        $written = @{}
        for ($c = 0; $c -lt $width; $c++) {
            $block = New-Object 'object[,]' $count, 1
            $textRows = New-Object System.Collections.ArrayList
            for ($i = 0; $i -lt $count; $i++) {
                $rowArgs = @($rows[$start + $i].args)
                if ($c -lt $rowArgs.Count) {
                    $value = Get-ArgumentValue $rowArgs[$c]
                    $block[$i, 0] = $value
                    $written["$i|$c"] = $value
                    if ($value -is [string]) { [void]$textRows.Add($i) }
                }
            }
            foreach ($i in $textRows) { $sheet.Cells.Item($i + 1, $c + 2).NumberFormat = '@' }
            $range = $sheet.Range($sheet.Cells.Item(1, $c + 2), $sheet.Cells.Item($count, $c + 2))
            $range.Value2 = $block
            # Optional transport audit: parse the same literal in a cell as in the
            # direct call, so the raw result retains its parsed binary64 value.
            for ($i = 0; $i -lt $count; $i++) {
                $auditRow = $rows[$start + $i]
                if ($null -ne $auditRow.PSObject.Properties['argument_literals'] -and $null -ne $auditRow.argument_literals -and $c -lt @($auditRow.argument_literals).Count) {
                    $sheet.Cells.Item($i + 1, $c + 2).Formula2R1C1 = '=' + [string]$auditRow.argument_literals[$c]
                }
            }
        }
        $runStart = 0
        for ($i = 1; $i -le $count; $i++) {
            if ($i -eq $count -or [string]$rows[$start + $i].formula -cne [string]$rows[$start + $runStart].formula) {
                $range = $sheet.Range($sheet.Cells.Item($runStart + 1, 1), $sheet.Cells.Item($i, 1))
                $range.Formula2R1C1 = [string]$rows[$start + $runStart].formula
                $runStart = $i
            }
        }
        Wait-Calculation $excel { $excel.Calculate() }
        $results = Read-Column ($sheet.Range($sheet.Cells.Item(1, 1), $sheet.Cells.Item($count, 1))) $count
        $readBack = @{}
        for ($c = 0; $c -lt $width; $c++) {
            $readBack[$c] = Read-Column ($sheet.Range($sheet.Cells.Item(1, $c + 2), $sheet.Cells.Item($count, $c + 2))) $count
        }
        for ($i = 0; $i -lt $count; $i++) {
            $kept = $true
            $submitted = New-Object System.Collections.ArrayList
            $actualInputs = New-Object System.Collections.ArrayList
            for ($c = 0; $c -lt @($rows[$start + $i].args).Count; $c++) {
                [void]$submitted.Add((Convert-Value $written["$i|$c"]))
                [void]$actualInputs.Add((Convert-Value $readBack[$c][$i]))
                if (-not (Test-Kept $written["$i|$c"] $readBack[$c][$i])) { $kept = $false }
            }
            $row = $rows[$start + $i]
            $literalInputs = $null
            if ($null -ne $row.PSObject.Properties['argument_literals']) { $literalInputs = $row.argument_literals }
            $audit = [ordered]@{
                requested = @($row.args); submitted_value2 = @($submitted); read_back = @($actualInputs)
                argument_literals = $literalInputs; formula = [string]$row.formula
                transport = $(if ($null -eq $literalInputs) { 'cell_value2_bulk' } else { 'formula_literals_with_cell_echo' })
                effective_callee_inputs = 'not directly observable'
            }
            [void]$Output.rows.Add([ordered]@{ id = [string]$row.id; value = (Convert-Value $results[$i]); input_kept = $kept; input_audit = $audit })
        }
        [void]$sheet.Cells.Clear()
    }

    $securityAfter = Get-ComProperty $excel 'AutomationSecurity'
    $Output.automation_security_after = [long]$securityAfter
    if ([long]$securityAfter -ne [long]$securityBefore) { throw 'AutomationSecurity changed during the reading' }
    $Output.excel_version = [string](Get-ComProperty $excel 'Version')
    $Output.excel_build = [string](Get-ComProperty $excel 'Build')
    $excelPath = (Get-Process -Id $excelPid).Path
    $version = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($excelPath)
    $Output.excel_file_version = '{0}.{1}.{2}.{3}' -f $version.FileMajorPart, $version.FileMinorPart, $version.FileBuildPart, $version.FilePrivatePart
    $Output.excel_machine = Get-Machine $excelPath
    $Output.os_version = [System.Environment]::OSVersion.VersionString
    try {
        $Output.cpu = (Get-CimInstance -ClassName Win32_Processor | Select-Object -First 1).Name
    } catch {
        Add-Failure 'environment' $null "the CPU could not be read: $($_.Exception.Message)"
    }
} catch {
    $Output.fatal = "$($_.Exception.Message) (witness-worker.ps1 line $($_.InvocationInfo.ScriptLineNumber))"
} finally {
    if ($null -ne $workbook) { try { $workbook.Close($false) } catch { } }
    if ($null -ne $excel -and $null -eq $OwnedExcel) {
        try { $excel.Quit() } catch { }
        try { [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($excel) } catch { }
    }
    $workbook = $null
    $excel = $null
    [System.GC]::Collect()
    [System.GC]::WaitForPendingFinalizers()
    [System.GC]::Collect()
}
$json = ConvertTo-Json -InputObject $Output -Depth 8 -Compress
[System.IO.File]::WriteAllText($Result, $json, (New-Object System.Text.UTF8Encoding($false)))
Send-Line 'done'
if ($null -ne $OwnedExcel) {
    if ($null -ne $Output.fatal) { throw $Output.fatal }
    return
}
if ($null -ne $Output.fatal) { exit 1 }
exit 0
