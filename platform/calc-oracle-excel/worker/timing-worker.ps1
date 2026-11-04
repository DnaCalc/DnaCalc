# The timing worker of calc-oracle-excel (bead dc-10d9): times Excel's own calculation of one
# workbook. The adapter (src/timing.rs) writes this script, and the capture worker whose functions
# it uses, into a fresh work directory and starts one Windows PowerShell process per workbook with a
# request file; it is not run by hand.
#
# It runs under the capture worker's isolation and reuses its functions (it dot-sources
# capture-worker.ps1, which then defines its functions and starts nothing): the package checks
# that keep Excel from ever opening a workbook with external content it could refresh, the new
# Excel instance whose PID it prints before anything else touches it, the session settings
# (DisplayAlerts, ScreenUpdating, EnableEvents and Interactive off), each read
# back, unchanged AutomationSecurity, and the add-in checks. It writes no option of the user's profile: it reads, and never sets,
# MultiThreadedCalculation, which Excel keeps in the profile.
#
# What it does, in order:
#   1. starts Excel as above and adds an empty workbook, in which it sets the calculation to manual:
#      a workbook opened later in the same instance then opens without calculating, so the open
#      time is that of loading the file, as DNA Calc's open is (Excel takes the calculation mode
#      from the first workbook of the session);
#   2. reads the process's working set with only that empty workbook open;
#   3. for each run: opens the input read-only with UpdateLinks 0 (timed), then times
#      CalculateFullRebuild, CalculateFull, Calculate with nothing dirty (noop_s), and, when the
#      request names a cell, two one-cell edits
#      of that cell: its Value2 set to the edited number (the number it held times 1.01, or 1 when
#      it held 0) and then back to the number it held, each
#      followed by Calculate, timing the assignment and the Calculate apart; it waits for
#      CalculationState to read xlDone (0) inside each timed call; then closes the workbook without
#      saving;
#   4. reads the Excel process's peak working set and peak private bytes (PeakWorkingSet64 and
#      PeakPagedMemorySize64 of Get-Process);
#   5. with the request's check_full (bead dc-dxm), shows what CalculateFull calculates: it writes a
#      copy of the input in which every formula cell's cached value is the sentinel number
#      -987654321 (Write-SentinelCopy), and, with multi-threaded calculation as the instance has
#      it and then switched off, opens the copy, counts the cells showing the sentinel (every
#      formula cell, when opening calculated nothing), times Calculate (nothing is dirty, so the
#      count stays) and CalculateFull, and counts again: a count of 0 after CalculateFull shows it
#      calculated every formula cell in the time measured. MultiThreadedCalculation.Enabled is
#      an option Excel keeps in the user's profile, so the value read before is set back in a
#      finally block before Excel quits;
#   6. quits Excel, writes the result file and prints "done".
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

$Sentinel = [double]-987654321

# A copy of the package at $Path, at $Out, in which every formula cell's cached value is $Sentinel
# (a number: any t attribute of the cell is dropped). Only worksheet parts change.
function Write-SentinelCopy([string]$Path, [string]$Out) {
    [System.IO.File]::Copy($Path, $Out, $true)
    $zip = [System.IO.Compression.ZipFile]::Open($Out, [System.IO.Compression.ZipArchiveMode]::Update)
    try {
        $names = @($zip.Entries | Where-Object { $_.FullName -like 'xl/worksheets/*.xml' } | ForEach-Object { $_.FullName })
        $cell = New-Object System.Text.RegularExpressions.Regex('<c( [^>]*?)?>(<f[^>]*/>|<f[^>]*>[^<]*</f>)(<v>[^<]*</v>|<v/>)?</c>')
        $type = New-Object System.Text.RegularExpressions.Regex(' t="[^"]*"')
        foreach ($name in $names) {
            $entry = $zip.GetEntry($name)
            $reader = New-Object System.IO.StreamReader($entry.Open(), (New-Object System.Text.UTF8Encoding($false)))
            $text = $reader.ReadToEnd()
            $reader.Close()
            $changed = $cell.Replace($text, [System.Text.RegularExpressions.MatchEvaluator] {
                param($m)
                '<c' + $type.Replace($m.Groups[1].Value, '') + '>' + $m.Groups[2].Value + '<v>-987654321</v></c>'
            })
            $entry.Delete()
            $entry = $zip.CreateEntry($name)
            $writer = New-Object System.IO.StreamWriter($entry.Open(), (New-Object System.Text.UTF8Encoding($false)))
            $writer.Write($changed)
            $writer.Close()
        }
    } finally {
        $zip.Dispose()
    }
}

# How many cells of the workbook's worksheets show $Sentinel, read through each used range's
# Value2 (one call per sheet).
function Get-SentinelCount($Workbook) {
    $count = 0
    foreach ($sheet in $Workbook.Worksheets) {
        $values = Get-ComProperty (Get-ComProperty $sheet 'UsedRange') 'Value2'
        if ($values -is [array]) {
            foreach ($value in $values) { if ($value -is [double] -and $value -eq $Sentinel) { $count++ } }
        } elseif ($values -is [double] -and $values -eq $Sentinel) {
            $count++
        }
    }
    return $count
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

    $securityBefore = Get-ComProperty $excel 'AutomationSecurity'
    if ($securityBefore -notin @(1, 2, 3)) { throw 'AutomationSecurity could not be read as a known value' }
    $Output['automation_security_before'] = $securityBefore
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
        $run = [ordered]@{ open_s = $null; rebuild_s = $null; full_s = $null; noop_s = $null; edits = (New-Object System.Collections.ArrayList); working_set = $null }
        $watch = [System.Diagnostics.Stopwatch]::StartNew()
        $workbook = Open-Workbook $excel $req.input
        $watch.Stop()
        $run.open_s = $watch.Elapsed.TotalSeconds
        $mode = Get-ComProperty $excel 'Calculation'
        if ([long]$mode -ne -4135) { throw ('the calculation mode reads as ' + (Get-Shown $mode) + ' after the workbook opened, not manual') }
        $run.rebuild_s = Measure-Calculation $excel { $excel.CalculateFullRebuild() }
        $run.full_s = Measure-Calculation $excel { $excel.CalculateFull() }
        # Calculate with nothing dirty: the floor of every Calculate, an edit's included.
        $run.noop_s = Measure-Calculation $excel { $excel.Calculate() }
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

    if ($req.check_full) {
        $copy = Join-Path (Split-Path -Parent $req.input) ('sentinel-' + (Split-Path -Leaf $req.input))
        Write-SentinelCopy $req.input $copy
        $Output.check = (New-Object System.Collections.ArrayList)
        $threaded = Get-Flag $mtc 'Enabled'
        try {
            foreach ($enabled in @($threaded, $false)) {
                $mtc.Enabled = $enabled
                $workbook = Open-Workbook $excel $copy
                $opened = Get-SentinelCount $workbook
                $calculate = Measure-Calculation $excel { $excel.Calculate() }
                $afterCalculate = Get-SentinelCount $workbook
                $full = Measure-Calculation $excel { $excel.CalculateFull() }
                $afterFull = Get-SentinelCount $workbook
                $again = Measure-Calculation $excel { $excel.CalculateFull() }
                [void]$Output.check.Add([ordered]@{ multithreaded = (Get-Flag $mtc 'Enabled'); thread_count = (Get-ComProperty $mtc 'ThreadCount'); sentinels_opened = $opened; calculate_s = $calculate; sentinels_after_calculate = $afterCalculate; full_s = $full; sentinels_after_full = $afterFull; full_again_s = $again })
                $workbook.Close($false)
                $workbook = $null
            }
        } finally {
            $mtc.Enabled = $threaded
        }
        $Output.multithreaded_restored = Get-Flag $mtc 'Enabled'
    }
    $Output['automation_security_after'] = Get-ComProperty $excel 'AutomationSecurity'
    if ($Output.automation_security_after -ne $securityBefore) { throw 'AutomationSecurity changed during timing' }
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
