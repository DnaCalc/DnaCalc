# The generator of the oracle workbook of dc-ylr (external workbook references) under
# crates/calc-host/tests/fixtures/dc_ylr/:
#
#   pwsh -NoProfile -File crates/calc-host/tests/fixtures/dc_ylr/make-dc-ylr-oracle.ps1
#       -WorkDir <directory> -OutDir <directory>
#
# It runs inside the repository's owned Excel session (tools/excel-session/submit.ps1), which
# launches an isolated `EXCEL.EXE /x` process and binds through that exact PID's native window.
# It never calls `New-Object -ComObject Excel.Application` (which would attach to a Touchstone
# broker worker) and never mutates Excel's session baseline settings or quits the shared session.
#
# In <WorkDir> it saves the source book ylr-source.xlsx (sheet Sheet1: 1, 2, 3 in A1:A3, "x", TRUE
# and =NA() in B1:B3, =A1*10 in D1; sheet "My Sheet": 10 in A1), closes it, and builds the linking
# book: every formula below entered with Range.Formula2 on sheet Main, typed with the source's full
# path ('<WorkDir>\[ylr-source.xlsx]Sheet1'!A1), and three defined names holding references to the
# source. Excel reads the closed source when the formulas are entered and saves what it read as
# the link's cache (xl/externalLinks/externalLink1.xml): the cells the formulas name that hold a
# value, so C1 (empty) is not cached. It saves the linking book as <OutDir>/oracle.xlsx and the
# same book as <OutDir>/oracle.xlsb. The capture reads the link's cache, as Excel does with its
# links not updated (the source is not at the path the link names).
#
# The source book Excel saved is also copied to <OutDir>/ylr-source.xlsx, beside the linking
# book. The corpus comparison resolves a link to a file of the link's name in the referring book's
# folder (tools/xtask-harness/src/linked_workbooks.rs) and calculates it, so the external
# references are calculated from the source rather than counted as external link caches.
# With -SourceOnly only that source is built and copied, and oracle.xlsx, oracle.xlsb and their
# capture stay as they are:
#
#   pwsh -NoProfile -File crates/calc-host/tests/fixtures/dc_ylr/make-dc-ylr-oracle.ps1
#       -WorkDir <directory> -OutDir crates/calc-host/tests/fixtures/dc_ylr -SourceOnly
#
#   cargo xtask capture --fixture crates/calc-host/tests/fixtures/dc_ylr/oracle.xlsx \
#       --out crates/calc-host/tests/captures/dc_ylr/oracle.capture.json --work-dir <short dir>
#
# Measured on Excel 16.0.20527 (2026-10-30), the source absent and links not updated: a cell the
# cache does not hold reads as blank (=X!C1 is 0, ISBLANK TRUE); an external reference is a
# reference (ISREF TRUE, AREAS, ROW, COLUMN, INDEX, unions); SUMIF, COUNTIF, SUMIFS, COUNTIFS,
# AVERAGEIF and OFFSET give #VALUE! for it, CELL, ISFORMULA and FORMULATEXT #N/A.

param(
    [Parameter(Mandatory = $true)][string]$WorkDir,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [switch]$SourceOnly,
    $OwnedExcel = $null,
    [string]$SessionDirectory = $env:DNACALC_EXCEL_SESSION
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
if (-not [System.IO.Path]::IsPathRooted($WorkDir)) { $WorkDir = Join-Path (Get-Location).Path $WorkDir }
if (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path (Get-Location).Path $OutDir }
$WorkDir = [System.IO.Path]::GetFullPath($WorkDir)
$OutDir = [System.IO.Path]::GetFullPath($OutDir)
New-Item -ItemType Directory -Force $WorkDir, $OutDir | Out-Null

# Direct invocation routes itself through the owned Excel session host (EXCEL.EXE /x, verified PID)
# rather than calling New-Object -ComObject Excel.Application.
if ($null -eq $OwnedExcel) {
    $repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
    $submitPath = (Resolve-Path -LiteralPath (Join-Path $repoRoot 'tools/excel-session/submit.ps1')).Path
    $jobParams = [ordered]@{
        WorkDir    = $WorkDir
        OutDir     = $OutDir
        SourceOnly = [bool]$SourceOnly
    }
    $submitArgs = @{
        Script           = $PSCommandPath
        Parameters       = ($jobParams | ConvertTo-Json -Compress)
        WorkingDirectory = $repoRoot
        TimeoutSeconds   = 900
        FreshRequired    = $true
    }
    if ($SessionDirectory) { $submitArgs.SessionDirectory = $SessionDirectory }
    & $submitPath @submitArgs
    $expected = @(Join-Path $OutDir 'ylr-source.xlsx')
    if (-not $SourceOnly) {
        $expected += (Join-Path $OutDir 'oracle.xlsx')
        $expected += (Join-Path $OutDir 'oracle.xlsb')
    }
    foreach ($path in $expected) {
        if (-not (Test-Path -LiteralPath $path)) { throw "Owned Excel job did not produce $path" }
    }
    return
}

# Retry transient COM busy rejections from the owned Excel instance.
function Retry([scriptblock]$Block) {
    for ($i = 0; $i -lt 120; $i++) {
        try { return & $Block } catch {
            if ($_.Exception.Message -match '0x80010001|0x8001010A|RPC_E_CALL_REJECTED|busy') { Start-Sleep -Milliseconds 500; continue }
            throw
        }
    }
    throw 'Excel rejected the call 120 times'
}

# X! and Y! stand for the source's Sheet1 and My Sheet.
$formulas = @(
    '=X!A1', '=X!B1', '=X!B2', '=X!B3', '=X!C1', '=X!D1', '=Y!A1', '=X!$A$1',
    '=SUM(X!A1:A3)', '=ROWS(X!A1:A3)', '=COLUMNS(X!A1:B3)', '=ISREF(X!A1)', '=ISREF(X!A1:A3)',
    '=INDEX(X!A1:A3,2)', '=INDEX(X!A1:B3,2,2)', '=ISREF(INDEX(X!A1:A3,2))', '=AREAS(X!A1:A3)',
    '=AREAS((X!A1,X!A3))', '=SUM((X!A1,X!A3))', '=SUMIF(X!A1:A3,">1")', '=COUNTIF(X!A1:A3,">1")',
    '=SUMIFS(X!A1:A3,X!A1:A3,">1")', '=COUNTIFS(X!A1:A3,">0")', '=AVERAGEIF(X!A1:A3,">1")',
    '=OFFSET(X!A1,1,0)', '=ISERROR(OFFSET(X!A1,0,0))', '=CELL("address",X!A1)', '=CELL("contents",X!A2)',
    '=ISFORMULA(X!D1)', '=FORMULATEXT(X!D1)', '=ISBLANK(X!C1)', '=COUNTBLANK(X!A1:C3)',
    '=COUNTA(X!A1:B3)', '=COUNT(X!A1:B3)', '=TYPE(X!A1:A3)', '=TYPE(X!A1)', '=COLUMN(X!B1)',
    '=ROW(X!A3)', '=SUMPRODUCT(X!A1:A3)', '=MATCH(2,X!A1:A3,0)', '=VLOOKUP(2,X!A1:B3,2,0)',
    '=HLOOKUP(1,X!A1:B3,2,0)', '=N(X!A1)', '=T(X!B1)', '=SUBTOTAL(9,X!A1:A3)', '=AGGREGATE(9,6,X!A1:B3)',
    '=ISNA(X!B3)', '=SUM(X!A1:A3,Y!A1)', '=X!A1+X!A2', '=ROWS(INDEX(X!A1:A3,0))', '=CHOOSE(2,X!A1,X!A2)',
    '=SUM(IF(TRUE,X!A1:A3))', '=ISREF(IF(TRUE,X!A1))', '=ISREF(CHOOSE(1,X!A1))', '=AVERAGE(X!A1:A3)',
    '=MIN(X!A1:A3)', '=SUM(X!A1:A3*2)', '=SUMPRODUCT((X!A1:A3>1)*X!A1:A3)', '=LET(r,X!A1:A3,SUM(r))',
    '=LET(r,X!A1:A3,ROWS(r))', '=SUM(X!A:A)', '=ROWS(X!A:A)', '=SUM(X!A1,X!A3)',
    '=ExtCell', '=SUM(ExtRange)', '=ROWS(ExtRange)', '=ExtSum', '=ISREF(ExtCell)', '=INDEX(ExtRange,3)',
    '=A1*100', '=1+1'
)
# Spilled results, each with room to spill.
$spills = [ordered]@{ 'C1' = '=X!A1:A3'; 'E1' = '=TRANSPOSE(X!A1:A3)'; 'C6' = '=X!A1:A3*2' }

$source = Join-Path $WorkDir 'ylr-source.xlsx'
$sourceCopy = Join-Path $OutDir 'ylr-source.xlsx'
Remove-Item -ErrorAction SilentlyContinue $source, $sourceCopy
if (-not $SourceOnly) { Remove-Item -ErrorAction SilentlyContinue (Join-Path $OutDir 'oracle.xlsx'), (Join-Path $OutDir 'oracle.xlsb') }

$prevAskToUpdateLinks = Retry { $OwnedExcel.AskToUpdateLinks }
$book = $null
try {
    Retry { $OwnedExcel.AskToUpdateLinks = $false } | Out-Null
    $book = Retry { $OwnedExcel.Workbooks.Add() }
    $sheet = $book.Worksheets.Item(1); $sheet.Name = 'Sheet1'
    $sheet.Range('A1').Value2 = 1; $sheet.Range('A2').Value2 = 2; $sheet.Range('A3').Value2 = 3
    $sheet.Range('B1').Value2 = 'x'; $sheet.Range('B2').Value2 = $true; $sheet.Range('B3').Formula = '=NA()'
    $sheet.Range('D1').Formula = '=A1*10'
    $other = $book.Worksheets.Add([Type]::Missing, $sheet); $other.Name = 'My Sheet'; $other.Range('A1').Value2 = 10
    # The owned session runs in manual calculation mode (-4135); calculate before saving so
    # ylr-source.xlsx stores Excel's calculated caches for B3 (=NA()) and D1 (=A1*10).
    Retry { $OwnedExcel.CalculateFull() } | Out-Null
    Retry { $book.SaveAs($source, 51) } | Out-Null
    Retry { $book.Close($false) } | Out-Null
    $book = $null
    Copy-Item -LiteralPath $source -Destination $sourceCopy -Force
    if ($SourceOnly) { return }

    $book = Retry { $OwnedExcel.Workbooks.Add() }
    $main = $book.Worksheets.Item(1); $main.Name = 'Main'
    $x = "'" + $WorkDir + '\[ylr-source.xlsx]Sheet1' + "'!"
    $y = "'" + $WorkDir + '\[ylr-source.xlsx]My Sheet' + "'!"
    [void]$book.Names.Add('ExtCell', "=$x`$A`$2")
    [void]$book.Names.Add('ExtRange', "=$x`$A`$1:`$A`$3")
    [void]$book.Names.Add('ExtSum', "=SUM($x`$A`$1:`$A`$3)")
    $row = 0
    foreach ($formula in $formulas) {
        $row++
        $text = $formula.Replace('X!', $x).Replace('Y!', $y)
        Retry { $main.Range("A$row").Formula2 = $text } | Out-Null
    }
    foreach ($at in $spills.Keys) {
        $text = $spills[$at].Replace('X!', $x).Replace('Y!', $y)
        Retry { $main.Range($at).Formula2 = $text } | Out-Null
    }
    Retry { $OwnedExcel.CalculateFull() } | Out-Null
    Retry { $book.SaveAs((Join-Path $OutDir 'oracle.xlsx'), 51) } | Out-Null
    Retry { $book.SaveAs((Join-Path $OutDir 'oracle.xlsb'), 50) } | Out-Null
    Retry { $book.Close($false) } | Out-Null
    $book = $null
} finally {
    if ($null -ne $book) {
        try { Retry { $book.Close($false) } | Out-Null } catch {}
    }
    try { Retry { $OwnedExcel.AskToUpdateLinks = $prevAskToUpdateLinks } | Out-Null } catch {}
}
