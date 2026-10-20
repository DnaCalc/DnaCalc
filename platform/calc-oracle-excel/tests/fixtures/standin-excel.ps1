# A fuller stand-in Excel for the worker's main-flow tests (src/worker_tests.rs), dot-sourced by
# standin-harness.ps1 as (part of) a test's setup: an Application with the settings and methods the
# main flow reads and calls, a workbook whose Sheet1 holds the formula cells of $global:StandinCells,
# whose SaveAs writes the package $global:StandinSaved to the output path (the input package when
# the setup names none), and which Workbooks.Open returns for the input and for the saved package
# alike. The collections are a small C# class, so they can be both enumerated and indexed as Excel's
# are. It stands for Excel only as far as the tests that use it say; what Excel itself does is the
# excel route's to show.
#
# Each formula cell is @(row, column, Formula2, Value2, spill), where spill is $null for a cell that
# does not spill (HasSpill False); @(relative address, rows, columns) for one that spills (HasSpill
# True, and a SpillingToRange whose Address(False, False) is that address, its absolute form for
# any other arguments, and whose Rows.Count and Columns.Count are those); 'unreadable-range' for
# one whose HasSpill is True and whose SpillingToRange cannot be read; or 'unreadable-hasspill' for
# one whose HasSpill cannot be read (a getter that throws, which PowerShell reads as nothing).
# SpecialCells(xlCellTypeFormulas) lists the formula cells and, after them, the cells
# @(row, column) of $global:StandinListed (none when the setup names none), standing for further
# cells Excel lists among its formula cells. A setup that sets these globals before this file gives
# its own; the default cells are the two of infra_m1_2/untrusted-link.xlsx, Sheet1!A1 and B1
# holding 111 and 112.
Add-Type -TypeDefinition @'
using System.Collections;
using System.Collections.Generic;
public class StandinItems : IEnumerable {
    private readonly List<object> order = new List<object>();
    private readonly Dictionary<string, object> named = new Dictionary<string, object>();
    public void Add(string key, object item) { order.Add(item); named[key] = item; }
    public object Item(object key) { if (key is int) { return order[(int)key - 1]; } return named[(string)key]; }
    public object Item(object row, object column) { return named[row + "," + column]; }
    public IEnumerator GetEnumerator() { return order.GetEnumerator(); }
}
'@
$global:StandinApplication = New-StandinApplication @() @()
$app = $global:StandinApplication
foreach ($pair in @(@('Calculation', -4105), @('Iteration', $false), @('MaxIterations', 100), @('MaxChange', 0.001), @('CalculationState', 0), @('Version', '16.0'), @('Build', 19127), @('OperatingSystem', 'Windows (64-bit) NT 10.00'), @('UseSystemSeparators', $true), @('CalculationVersion', 191029), @('AskToUpdateLinks', $true), @('IgnoreRemoteRequests', $false))) {
    $app | Add-Member -MemberType NoteProperty -Name $pair[0] -Value $pair[1]
}
$app | Add-Member -MemberType ScriptMethod -Name CalculateFullRebuild -Value { Write-Standin 'CalculateFullRebuild' }
$app | Add-Member -MemberType ScriptMethod -Name International -Value { param($index) return [string]$index }
$languages = [pscustomobject]@{}
$languages | Add-Member -MemberType ScriptMethod -Name LanguageID -Value { param($kind) return 1033 }
$app | Add-Member -MemberType NoteProperty -Name LanguageSettings -Value $languages
if ($null -eq $global:StandinCells) {
    $global:StandinCells = @(@(1, 1, '=[1]Sheet1!$A$1', 111.0, $null), @(1, 2, '=A1+1', 112.0, $null))
}
$cells = New-Object StandinItems
$formulaCells = @()
foreach ($spec in $global:StandinCells) {
    $cell = [pscustomobject]@{ Row = $spec[0]; Column = $spec[1]; HasFormula = $true; Formula2 = $spec[2]; Value2 = $spec[3] }
    $spill = $spec[4]
    if ($null -eq $spill) {
        $cell | Add-Member -MemberType NoteProperty -Name HasSpill -Value $false
    } elseif ($spill -is [string] -and $spill -eq 'unreadable-hasspill') {
        $cell | Add-Member -MemberType ScriptProperty -Name HasSpill -Value { throw 'the stand-in cannot read HasSpill' }
    } elseif ($spill -is [string] -and $spill -eq 'unreadable-range') {
        $cell | Add-Member -MemberType NoteProperty -Name HasSpill -Value $true
        $cell | Add-Member -MemberType ScriptProperty -Name SpillingToRange -Value { throw 'the stand-in cannot read SpillingToRange' }
    } else {
        $range = [pscustomobject]@{ Relative = $spill[0]; Rows = [pscustomobject]@{ Count = $spill[1] }; Columns = [pscustomobject]@{ Count = $spill[2] } }
        $range | Add-Member -MemberType ScriptMethod -Name Address -Value {
            param($RowAbsolute = $true, $ColumnAbsolute = $true)
            if ($RowAbsolute -or $ColumnAbsolute) { return ($this.Relative -replace '([A-Z]+)([0-9]+)', '$$$1$$$2') }
            return $this.Relative
        }
        $cell | Add-Member -MemberType NoteProperty -Name HasSpill -Value $true
        $cell | Add-Member -MemberType NoteProperty -Name SpillingToRange -Value $range
    }
    $cells.Add(([string]$spec[0] + ',' + [string]$spec[1]), $cell)
    $formulaCells += $cell
}
$listedCells = $formulaCells
if ($null -ne $global:StandinListed) {
    foreach ($place in $global:StandinListed) { $listedCells += [pscustomobject]@{ Row = $place[0]; Column = $place[1] } }
}
$used = [pscustomobject]@{ HasFormula = $true }
$used | Add-Member -MemberType NoteProperty -Name ListedCells -Value $listedCells
$used | Add-Member -MemberType ScriptMethod -Name SpecialCells -Value { param($type) return [pscustomobject]@{ Cells = $this.ListedCells } }
$sheet = [pscustomobject]@{ Name = 'Sheet1'; Cells = $cells; UsedRange = $used }
$sheets = New-Object StandinItems
$sheets.Add('Sheet1', $sheet)
$book = [pscustomobject]@{ Worksheets = $sheets; Connections = @(); CompatibilityVersion = 16; CalculationVersion = 191029; Date1904 = $false; PrecisionAsDisplayed = $false; HasVBProject = $false }
$book | Add-Member -MemberType ScriptMethod -Name LinkSources -Value { param($kind) if ($kind -eq 1) { return , @('C:\nowhere\link-target.xlsx') } return $null }
$book | Add-Member -MemberType ScriptMethod -Name SaveAs -Value {
    param($path, $format)
    $from = $global:StandinSaved
    if ($null -eq $from) { $from = $global:StandinInput }
    Write-Standin ("SaveAs $format from " + [System.IO.Path]::GetFileName($from))
    Copy-Item -LiteralPath $from -Destination $path
}
$book | Add-Member -MemberType ScriptMethod -Name Close -Value { Write-Standin 'Close' }
$global:StandinBook = $book
$global:StandinInput = $null
$app.Workbooks | Add-Member -MemberType ScriptMethod -Name Open -Value { Write-Standin ('Workbooks.Open ' + [System.IO.Path]::GetFileName($args[0]) + ' from ' + [System.IO.Path]::GetFileName([System.IO.Path]::GetDirectoryName($args[0]))); if ($null -eq $global:StandinInput) { $global:StandinInput = $args[0] }; return $global:StandinBook } -Force
