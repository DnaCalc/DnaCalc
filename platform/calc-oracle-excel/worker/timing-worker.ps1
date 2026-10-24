# The timing worker of calc-oracle-excel (bead dc-10d9): times Excel's own calculation of one
# workbook. The adapter (src/timing.rs) writes this script, and the capture worker whose functions
# it uses, into a fresh work directory and starts one Windows PowerShell process per workbook with a
# request file; it is not run by hand.
#
# It runs under the capture worker's isolation and reuses its functions (it dot-sources
# capture-worker.ps1, which then defines its functions and starts nothing): the package checks
# that keep Excel from ever opening a workbook with external content it could refresh, the new
# Excel instance whose PID it prints before anything else touches it, the session settings
# (DisplayAlerts, ScreenUpdating, EnableEvents and Interactive off, AutomationSecurity 3), each read
# back, and the add-in checks. It writes no option of the user's profile: it reads, and never sets,
# MultiThreadedCalculation, which Excel keeps in the profile.
#
# What it does, in order:
#   1. starts Excel as above and adds an empty workbook, in which it sets the calculation to manual:
#      a workbook opened later in the same instance then opens without calculating, so the open
#      time is that of loading the file, as DNA Calc's open is (Excel takes the calculation mode
#      from the first workbook of the session);
#   2. reads the process's working set with only that empty workbook open;
#   3. for each run: opens the input read-only with UpdateLinks 0 (timed), then times
#      CalculateFullRebuild, CalculateFull, and, when the request names a cell, two one-cell edits
#      of that cell: its Value2 set to the edited number (the number it held times 1.01, or 1 when
#      it held 0) and then back to the number it held, each
#      followed by Calculate, timing the assignment and the Calculate apart; it waits for
#      CalculationState to read xlDone (0) inside each timed call; then closes the workbook without
#      saving;
#   4. reads the Excel process's peak working set and peak private bytes (PeakWorkingSet64 and
#      PeakPagedMemorySize64 of Get-Process), quits Excel, writes the result file and prints "done".
#
# Times are of a [System.Diagnostics.Stopwatch] around the one COM call, in seconds. The script is
# ASCII and runs under Windows PowerShell 5.1.

param(
    [string]$Request,
    [string]$Result
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

# Dot-sourcing runs the capture worker's param block in this scope, which binds its -Request and
# -Result to nothing, so this script's own are kept apart first.
$TimingRequest = $Request
$TimingResult = $Result
. (Join-Path $PSScriptRoot 'capture-worker.ps1')
$Request = $TimingRequest
$Result = $TimingResult

if ([string]::IsNullOrEmpty($Request) -or [string]::IsNullOrEmpty($Result)) {
    [Console]::Error.WriteLine('timing-worker.ps1 needs -Request <request.json> and -Result <result.json>')
    exit 2
}

$Output = [ordered]@{
    format = 'dnacalc-timing-worker-result'
    version = 1
    excel_pid = $null
    fatal = $null
    excel_build = $null
    multithreaded = $null
    thread_count = $null
    empty_working_set = $null
    peak_working_set = $null
    peak_private = $null
    runs = (New-Object System.Collections.ArrayList)
    failures = $Failures
}

# Calls $Action, then waits for Application.CalculationState to read xlDone (0), and returns the
# seconds both took.
function Measure-Calculation($Application, [scriptblock]$Action) {
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    & $Action
    while ($true) {
        $state = Get-ComProperty $Application 'CalculationState'
        if (-not ($state -is [int] -or $state -is [long])) { throw ('CalculationState reads as ' + (Get-Shown $state)) }
        if ([long]$state -eq 0) { break }
        Start-Sleep -Milliseconds 1
    }
    $watch.Stop()
    return $watch.Elapsed.TotalSeconds
}

$excel = $null
$workbook = $null
$blank = $null
try {
    $req = [System.IO.File]::ReadAllText($Request, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
    $package = Read-Package $req.input
    $refusals = Get-PackageRefusals $package
    if ($refusals.Count -gt 0) {
        foreach ($refusal in $refusals) { Add-Failure 'isolation' $null "$refusal; Excel was not started, so the workbook was never opened" }
        throw ('refused before Excel was started: the package declares external content Excel could refresh or activate: ' + ($refusals -join '; '))
    }

    $excel = New-Object -ComObject Excel.Application
    $excelPid = Get-ExcelPid $excel
    $Output.excel_pid = $excelPid
    Send-Line "excel-pid $excelPid"

    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $excel.ScreenUpdating = $false
    $excel.EnableEvents = $false
    $excel.AutomationSecurity = 3
    $excel.Interactive = $false
    $expected = [ordered]@{ DisplayAlerts = $false; EnableEvents = $false; Interactive = $false; AutomationSecurity = 3 }
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
        throw ('refused before the workbook was opened: this Excel instance is not isolated: ' + ($notIsolated -join '; '))
    }
    $Output.excel_build = (Get-Item -LiteralPath (Get-Process -Id $excelPid).Path).VersionInfo.FileVersion
    $mtc = Get-ComProperty $excel 'MultiThreadedCalculation'
    $Output.multithreaded = Get-Flag $mtc 'Enabled'
    $Output.thread_count = Get-ComProperty $mtc 'ThreadCount'

    $blank = Invoke-ComMethod $excel.Workbooks 'Add' @()
    $excel.Calculation = -4135
    $excel.CalculateBeforeSave = $false
    $calculation = Get-ComProperty $excel 'Calculation'
    if ([long]$calculation -ne -4135) { throw ('Calculation reads as ' + (Get-Shown $calculation) + ' after it was set to manual (-4135)') }
    $process = Get-Process -Id $excelPid
    $process.Refresh()
    $Output.empty_working_set = $process.WorkingSet64

    for ($i = 0; $i -lt [int]$req.runs; $i++) {
        $run = [ordered]@{ open_s = $null; rebuild_s = $null; full_s = $null; edits = (New-Object System.Collections.ArrayList); working_set = $null }
        $watch = [System.Diagnostics.Stopwatch]::StartNew()
        $workbook = Open-Workbook $excel $req.input
        $watch.Stop()
        $run.open_s = $watch.Elapsed.TotalSeconds
        $mode = Get-ComProperty $excel 'Calculation'
        if ([long]$mode -ne -4135) { throw ('the calculation mode reads as ' + (Get-Shown $mode) + ' after the workbook opened, not manual') }
        $run.rebuild_s = Measure-Calculation $excel { $excel.CalculateFullRebuild() }
        $run.full_s = Measure-Calculation $excel { $excel.CalculateFull() }
        if ($null -ne $req.edit) {
            $cell = $workbook.Worksheets.Item([string]$req.edit.sheet).Cells.Item([int]$req.edit.row, [int]$req.edit.column)
            if ([bool](Get-ComProperty $cell 'HasFormula')) { throw "the edited cell holds a formula, not an input" }
            $held = Get-ComProperty $cell 'Value2'
            if ($held -isnot [double]) { throw ('the edited cell holds ' + (Get-Shown $held) + ', not a number') }
            # The edited number: the number held times 1.01, or 1 when it holds 0 (the rule DNA
            # Calc's bench applies too).
            if ($held -ne 0) { $edited = $held * 1.01 } else { $edited = [double]1 }
            foreach ($target in @($edited, $held)) {
                $watch = [System.Diagnostics.Stopwatch]::StartNew()
                $cell.Value2 = $target
                $watch.Stop()
                $assign = $watch.Elapsed.TotalSeconds
                $calculate = Measure-Calculation $excel { $excel.Calculate() }
                [void]$run.edits.Add([ordered]@{ from_bits = [BitConverter]::DoubleToInt64Bits($held).ToString('x16'); to_bits = [BitConverter]::DoubleToInt64Bits($target).ToString('x16'); assign_s = $assign; calculate_s = $calculate })
            }
        }
        $process.Refresh()
        $run.working_set = $process.WorkingSet64
        $workbook.Close($false)
        $workbook = $null
        [void]$Output.runs.Add($run)
    }
    $process.Refresh()
    $Output.peak_working_set = $process.PeakWorkingSet64
    $Output.peak_private = $process.PeakPagedMemorySize64
} catch {
    $Output.fatal = "$($_.Exception.Message) (timing-worker.ps1 line $($_.InvocationInfo.ScriptLineNumber))"
} finally {
    if ($null -ne $workbook) { try { $workbook.Close($false) } catch { } }
    if ($null -ne $blank) { try { $blank.Close($false) } catch { } }
    if ($null -ne $excel) {
        try { $excel.Quit() } catch { }
        try { [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($excel) } catch { }
    }
    $workbook = $null
    $blank = $null
    $excel = $null
    [System.GC]::Collect()
    [System.GC]::WaitForPendingFinalizers()
    [System.GC]::Collect()
}
$json = ConvertTo-Json -InputObject $Output -Depth 8 -Compress
[System.IO.File]::WriteAllText($Result, $json, (New-Object System.Text.UTF8Encoding($false)))
Send-Line 'done'
if ($null -ne $Output.fatal) { exit 1 }
exit 0
