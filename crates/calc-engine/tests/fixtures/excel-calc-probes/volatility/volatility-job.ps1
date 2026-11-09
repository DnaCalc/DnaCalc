param([string]$SpecFile, [string]$OutDirectory, [string]$Only = '', [int]$TimeoutSeconds = 1800, [switch]$FreshRequired, $OwnedExcel = $null)
# Volatility probes (vol-1). One job runs every probe of the spec file in an owned hidden Excel
# session (tools/excel-session); run it through on-private-desktop.ps1 so no window reaches the
# owner's desktop. Run without -OwnedExcel it submits itself to the session queue named by
# DNACALC_EXCEL_SESSION.
#
# Spec (JSON): { "xll": path|null, "probes": [ { "id", "trials", "xll_cfg": [[name,script]],
#   "vba_cfg": [[name,script]], "consts": [[addr,value]], "names": [[name,refersTo]],
#   "cells": [[addr,formula]], "watch": [addr...], "steps": [ {op,...} ] } ] }
# Step ops: calc | full | rebuild | sheet | range(addr) | set(addr,value) | setf(addr,formula) |
#   name(name,refers) | xcfg(name,script) | vcfg(name,script) | read(tag) | reset | save(tag) |
#   manual | auto | dirty(addr) | tag(tag) | savexlsx(tag, last step) | reopen(tag)
# Each calc-like op and each read records: the VBA call counts keyed name@caller, the XLL log,
# and the watched cell values, since the previous record; then both logs are cleared.
# The book is first saved as .xlsb so SaveCopyAs keeps that format; CalculateBeforeSave is off for
# the job and restored; nothing else in Excel is configured.
$ErrorActionPreference = 'Stop'
if ($null -eq $OwnedExcel) {
    $parameters = @{ SpecFile = $SpecFile; OutDirectory = $OutDirectory; Only = $Only }
    $client = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../../tools/excel-session/submit.ps1')).Path
    $arguments = @{ Script = $PSCommandPath; Parameters = ($parameters | ConvertTo-Json -Compress); Environment = 'vol1:volatility'; TimeoutSeconds = $TimeoutSeconds; LeakEvery = 1000000 }
    if ($FreshRequired) { $arguments.FreshRequired = $true }
    & $client @arguments
    return
}
$excel = $OwnedExcel
New-Item -ItemType Directory -Force -Path $OutDirectory | Out-Null
$spec = Get-Content -LiteralPath $SpecFile -Raw | ConvertFrom-Json
$module = ([IO.File]::ReadAllText((Join-Path $PSScriptRoot 'Probe.bas')) -replace "`r?`n", "`r`n") -replace '(?m)^Attribute VB_Name.*\r\n', ''
$result = [ordered]@{
    excel_version = [string]$excel.Version; excel_build = [string]$excel.Build
    calculation_version = $excel.CalculationVersion
    mtc_enabled = $excel.MultiThreadedCalculation.Enabled; mtc_threads = $excel.MultiThreadedCalculation.ThreadCount
    automation_security = $excel.AutomationSecurity; started_utc = [DateTime]::UtcNow.ToString('O')
    xll = $spec.xll; rows = @(); errors = @()
}
$beforeSave = $null
$xllLoaded = $false
if ($spec.xll) {
    try { $xllLoaded = [bool]$excel.RegisterXLL([string]$spec.xll); $result.xll_registered = $xllLoaded }
    catch { $result.errors += ('RegisterXLL: ' + $_.Exception.Message) }
}
function Fmt($v) { if ($null -eq $v) { return $null } if ($v -is [double]) { return ('{0:R}' -f $v) } return [string]$v }
function RunMacro($book, [string]$name) {
    try { $r = $excel.Run("'" + $book.Name + "'!" + $name); if ($null -eq $r) { return '' } return [string]$r } catch { if ($_.Exception.Message -like "*Parameter 'ptr'*") { return '' } return 'ERR:' + $_.Exception.Message }
}
function RunXll([string]$name) {
    try { $r = $excel.Run($name); if ($null -eq $r) { return '' } return [string]$r } catch { if ($_.Exception.Message -like "*Parameter 'ptr'*") { return '' } return 'ERR:' + $_.Exception.Message }
}
function Record($book, $sheet, $probe, [string]$tag, [string]$op) {
    $record = [ordered]@{ tag = $tag; op = $op; counts = (RunMacro $book 'GetCounts') }
    if ($script:xllLoaded) { $record.xll = (RunXll 'VolLog') }
    $values = [ordered]@{}
    foreach ($a in @($probe.watch)) { if ($a) { try { $values[$a] = Fmt $sheet.Range($a).Value2 } catch { $values[$a] = 'ERR' } } }
    $record.values = $values
    [void](RunMacro $book 'VReset')
    if ($script:xllLoaded) { [void](RunXll 'VolReset') }
    return $record
}
try {
    foreach ($probe in $spec.probes) {
        if ($Only -and $probe.id -ne $Only) { continue }
        $trials = if ($probe.trials) { [int]$probe.trials } else { 3 }
        for ($trial = 1; $trial -le $trials; $trial++) {
            $row = [ordered]@{ probe = $probe.id; trial = $trial; records = @(); saves = @() }
            $book = $null
            $step = 'add'
            try {
                $book = $excel.Workbooks.Add()
                $step = 'calculation'; $excel.Calculation = -4135
                if ($null -eq $beforeSave) { $beforeSave = $excel.CalculateBeforeSave }
                $excel.CalculateBeforeSave = $false
                $step = 'vbproject'
                $component = $book.VBProject.VBComponents.Add(1)
                $component.Name = 'Probe'
                $component.CodeModule.AddFromString($module)
                $tdir = Join-Path $OutDirectory ($probe.id + '_t' + $trial)
                New-Item -ItemType Directory -Force -Path $tdir | Out-Null
                $step = 'saveas'
                $book.SaveAs((Join-Path $tdir 'book.xlsb'), 50)
                $sheet = $book.Worksheets.Item(1)
                $step = 'cfg'
                foreach ($c in @($probe.vba_cfg)) { if ($c) { [void]$excel.Run("'" + $book.Name + "'!VCfg", [string]$c[0], [string]$c[1]) } }
                if ($xllLoaded) { foreach ($c in @($probe.xll_cfg)) { if ($c) { [void]$excel.Run('VolCfg', [string]$c[0], [string]$c[1]) } } }
                [void](RunMacro $book 'VResetCalls'); if ($xllLoaded) { [void](RunXll 'VolResetCalls') }
                $step = 'consts'
                foreach ($c in @($probe.consts)) { if ($c) { if ($c[1] -is [string]) { $sheet.Range($c[0]).Value = [string]$c[1] } else { $sheet.Range($c[0]).Value2 = [double]$c[1] } } }
                $step = 'names'
                $row.name_errors = @(); foreach ($n in @($probe.names)) { if ($n) { try { [void]$book.Names.Add([string]$n[0], [string]$n[1]) } catch { $row.name_errors += ($n[0] + ': ' + $_.Exception.Message) } } }
                $step = 'enter'
                $row.entry_errors = @(); foreach ($c in @($probe.cells)) { if ($c) { try { $sheet.Range($c[0]).Formula2 = [string]$c[1] } catch { $row.entry_errors += ($c[0] + ': ' + $_.Exception.Message) } } }
                $row.formulas = @($probe.cells | ForEach-Object { $_[0] + ' ' + $_[1] })
                $row.records += (Record $book $sheet $probe 'entry' 'enter')
                $saveIndex = 0
                foreach ($s in @($probe.steps)) {
                    $step = 'step ' + $s.op
                    $tag = if ($s.tag) { [string]$s.tag } else { [string]$s.op }
                    switch ($s.op) {
                        'calc' { $excel.Calculate(); $row.records += (Record $book $sheet $probe $tag 'calc') }
                        'full' { $excel.CalculateFull(); $row.records += (Record $book $sheet $probe $tag 'full') }
                        'rebuild' { $excel.CalculateFullRebuild(); $row.records += (Record $book $sheet $probe $tag 'rebuild') }
                        'sheet' { $sheet.Calculate(); $row.records += (Record $book $sheet $probe $tag 'sheet') }
                        'range' { $sheet.Range([string]$s.addr).Calculate(); $row.records += (Record $book $sheet $probe $tag ('range ' + $s.addr)) }
                        'set' { $sheet.Range([string]$s.addr).Value2 = $s.value }
                        'setf' { $sheet.Range([string]$s.addr).Formula2 = [string]$s.formula }
                        'dirty' { $sheet.Range([string]$s.addr).Dirty() }
                        'name' { $book.Names.Item([string]$s.name).RefersTo = [string]$s.refers }
                        'xcfg' { [void]$excel.Run('VolCfg', [string]$s.name, [string]$s.script) }
                        'vcfg' { [void]$excel.Run("'" + $book.Name + "'!VCfg", [string]$s.name, [string]$s.script) }
                        'read' { $row.records += (Record $book $sheet $probe $tag 'read') }
                        'reset' { [void](RunMacro $book 'VReset'); if ($xllLoaded) { [void](RunXll 'VolReset') } }
                        'resetcalls' { [void](RunMacro $book 'VResetCalls'); if ($xllLoaded) { [void](RunXll 'VolResetCalls') } }
                        'manual' { $excel.Calculation = -4135 }
                        'auto' { $excel.Calculation = -4105; $row.records += (Record $book $sheet $probe $tag 'auto') }
                        'save' { $path = Join-Path $tdir ($tag + '.xlsb'); $book.SaveCopyAs($path); $row.saves += $tag }
                        'savexlsx' { $path = Join-Path $tdir ($tag + '.xlsx'); $excel.DisplayAlerts = $false; $book.SaveAs($path, 51); $row.saves += ($tag + '.xlsx') }
                        'run' { $row.records += [ordered]@{ tag = $tag; op = 'run'; macro = [string]$s.macro; ret = (RunMacro $book ([string]$s.macro)) } }
                        'xrun' { $row.records += [ordered]@{ tag = $tag; op = 'xrun'; name = [string]$s.name; ret = (RunXll ([string]$s.name)); xll = (RunXll 'VolLog') }; [void](RunXll 'VolReset') }
                        'eval' { $ev = $null; try { $ev = [string]$excel.Evaluate([string]$s.expr) } catch { $ev = 'ERR:' + $_.Exception.Message }; $row.records += [ordered]@{ tag = $tag; op = 'eval'; expr = [string]$s.expr; ret = $ev; counts = (RunMacro $book 'GetCounts') }; [void](RunMacro $book 'VReset') }
                        'reopen' {
                            $path = Join-Path $tdir ([string]$s.file + '.xlsb')
                            $old = $book
                            $book = $excel.Workbooks.Open($path)
                            $old.Close($false)
                            $sheet = $book.Worksheets.Item(1)
                            $row.records += (Record $book $sheet $probe $tag 'open')
                        }
                        'tag' { }
                        default { throw ('unknown op ' + $s.op) }
                    }
                }
            } catch { $row.error = $_.Exception.Message; $row.step = $step }
            finally { if ($book) { try { if ($null -ne $beforeSave) { $excel.CalculateBeforeSave = $beforeSave } } catch { $row.restore_error = $_.Exception.Message }; try { $book.Close($false) } catch { $row.close_error = $_.Exception.Message } } }
            $result.rows += $row
        }
    }
} finally {
    $result.finished_utc = [DateTime]::UtcNow.ToString('O')
    $name = if ($Only) { 'volatility-' + $Only + '.json' } else { 'volatility-' + [IO.Path]::GetFileNameWithoutExtension($SpecFile) + '.json' }
    $result | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $OutDirectory $name) -Encoding utf8NoBOM
}
