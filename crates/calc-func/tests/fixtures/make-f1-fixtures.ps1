# The generator of F1's (dc-8dq) workbook fixtures under crates/calc-func/tests/fixtures/.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-f1-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/f1_prefix/oracle.xlsx. Run with this directory as -OutDir it rewrites the
# committed fixture; the acceptance case dc_xrxb__prefix_metadata_matches_excel runs it into a
# fresh directory on Windows and compares its output with the committed workbook byte for byte.
#
# f1_prefix/oracle.xlsx is the workbook the Excel capture of F1's file prefixes reads
# (`cargo xtask capture --fixture crates/calc-func/tests/fixtures/f1_prefix/oracle.xlsx --case
# f1__prefix_metadata ...`), which also keeps the package Excel saves from it. It holds the rows
# of the functions F1 catalogues, in the layout of dc_xrxb/oracle.xlsx (see make-fixtures.ps1),
# so that make-fixtures.ps1, which other lanes extend, stays as it is.
#
# The package writer below is make-fixtures.ps1's, copied: a minimal SpreadsheetML package written
# as text, every part stored with the timestamp 1980-01-01 00:00, so the bytes depend on nothing
# but this script. The script is ASCII and runs under Windows PowerShell 5.1.

param(
    [Parameter(Mandatory = $true)][string]$OutDir
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
if (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path (Get-Location).Path $OutDir }

$Main = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'
$Rel = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
$PkgRel = 'http://schemas.openxmlformats.org/package/2006/relationships'
$ContentTypes = 'http://schemas.openxmlformats.org/package/2006/content-types'
$Decl = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' + "`n"

# --- the ZIP writer ------------------------------------------------------------------------------

$CrcTable = New-Object 'int64[]' 256
for ($n = 0; $n -lt 256; $n++) {
    [int64]$c = $n
    for ($k = 0; $k -lt 8; $k++) {
        if ($c -band 1) { $c = 3988292384 -bxor ($c -shr 1) } else { $c = $c -shr 1 }
    }
    $CrcTable[$n] = $c
}

# The CRC-32 (ISO-HDLC, as ZIP uses) of the bytes.
function Get-Crc32([byte[]]$Bytes) {
    [int64]$crc = 4294967295
    foreach ($b in $Bytes) { $crc = $CrcTable[($crc -bxor $b) -band 255] -bxor ($crc -shr 8) }
    return [uint32]($crc -bxor 4294967295)
}

# Writes a ZIP file of the parts, in the order given (an array of @(name, text)): each entry
# stored, dated 1980-01-01 00:00 (DOS date 0x0021, time 0), with no extra field and no comment.
function Write-Package([string]$Path, $Parts) {
    $stream = New-Object System.IO.MemoryStream
    $writer = New-Object System.IO.BinaryWriter($stream)
    $central = New-Object System.Collections.ArrayList
    foreach ($part in $Parts) {
        $name = [System.Text.Encoding]::ASCII.GetBytes($part[0])
        $data = (New-Object System.Text.UTF8Encoding($false)).GetBytes($part[1])
        $crc = Get-Crc32 $data
        $offset = [uint32]$stream.Position
        $writer.Write([uint32]0x04034b50)
        $writer.Write([uint16]20); $writer.Write([uint16]0); $writer.Write([uint16]0)
        $writer.Write([uint16]0); $writer.Write([uint16]0x0021)
        $writer.Write([uint32]$crc); $writer.Write([uint32]$data.Length); $writer.Write([uint32]$data.Length)
        $writer.Write([uint16]$name.Length); $writer.Write([uint16]0)
        $writer.Write($name); $writer.Write($data)
        [void]$central.Add(@($name, $crc, [uint32]$data.Length, $offset))
    }
    $start = [uint32]$stream.Position
    foreach ($entry in $central) {
        $writer.Write([uint32]0x02014b50)
        $writer.Write([uint16]20); $writer.Write([uint16]20); $writer.Write([uint16]0); $writer.Write([uint16]0)
        $writer.Write([uint16]0); $writer.Write([uint16]0x0021)
        $writer.Write([uint32]$entry[1]); $writer.Write([uint32]$entry[2]); $writer.Write([uint32]$entry[2])
        $writer.Write([uint16]$entry[0].Length); $writer.Write([uint16]0); $writer.Write([uint16]0)
        $writer.Write([uint16]0); $writer.Write([uint16]0); $writer.Write([uint32]0)
        $writer.Write([uint32]$entry[3])
        $writer.Write([byte[]]$entry[0])
    }
    $size = [uint32]$stream.Position - $start
    $writer.Write([uint32]0x06054b50)
    $writer.Write([uint16]0); $writer.Write([uint16]0)
    $writer.Write([uint16]$central.Count); $writer.Write([uint16]$central.Count)
    $writer.Write([uint32]$size); $writer.Write([uint32]$start); $writer.Write([uint16]0)
    $writer.Flush()
    $directory = Split-Path -Parent $Path
    if (-not (Test-Path -LiteralPath $directory)) { [void](New-Item -ItemType Directory -Path $directory) }
    [System.IO.File]::WriteAllBytes($Path, $stream.ToArray())
}

# --- the package builder ------------------------------------------------------------------------

function Get-Escaped([string]$Text) {
    return $Text.Replace('&', '&amp;').Replace('<', '&lt;').Replace('>', '&gt;')
}

# Constant cells, each @(reference, XML).
function New-Number([string]$Ref, [string]$Value) { return @($Ref, ('<c r="' + $Ref + '"><v>' + $Value + '</v></c>')) }
function New-Text([string]$Ref, [string]$Text) { return @($Ref, ('<c r="' + $Ref + '" t="inlineStr"><is><t>' + (Get-Escaped $Text) + '</t></is></c>')) }
function New-Logical([string]$Ref, [bool]$Value) {
    $v = '0'
    if ($Value) { $v = '1' }
    return @($Ref, ('<c r="' + $Ref + '" t="b"><v>' + $v + '</v></c>'))
}
function New-Error([string]$Ref, [string]$Literal) { return @($Ref, ('<c r="' + $Ref + '" t="e"><v>' + $Literal + '</v></c>')) }

# The row number and column letters of an A1 reference.
function Split-Ref([string]$Ref) {
    $letters = $Ref -replace '[0-9]', ''
    return @([int]($Ref.Substring($letters.Length)), $letters)
}

# A worksheet part: the constant cells $Inputs (each @(reference, XML), in any column but D), the
# formulas $Formulas, one per row of column D from row 1, and the source formulas $Sources (each
# @(reference, formula as the file stores it), in any column but D), each written as a
# dynamic-array formula over its one cell (cm="1", the metadata part's one cell metadata block, so
# the workbook must be written with -DynamicArrays). Rows are written in order, each row's cells in
# column order.
function New-OracleSheet($Inputs, $Formulas, $Sources = @()) {
    $rows = @{}
    foreach ($cell in $Inputs) {
        $place = Split-Ref $cell[0]
        if ($place[1] -eq 'D') { throw "input cell $($cell[0]) is in the formula column D" }
        if (-not $rows.ContainsKey($place[0])) { $rows[$place[0]] = @{} }
        $rows[$place[0]][$place[1]] = $cell[1]
    }
    foreach ($source in $Sources) {
        $place = Split-Ref $source[0]
        if ($place[1] -eq 'D') { throw "source cell $($source[0]) is in the formula column D" }
        if (-not $rows.ContainsKey($place[0])) { $rows[$place[0]] = @{} }
        if ($rows[$place[0]].ContainsKey($place[1])) { throw "cell $($source[0]) is written twice" }
        $rows[$place[0]][$place[1]] = '<c r="' + $source[0] + '" cm="1"><f t="array" ref="' + $source[0] + '">' + (Get-Escaped $source[1]) + '</f></c>'
    }
    $number = 0
    foreach ($formula in $Formulas) {
        $number += 1
        if (-not $rows.ContainsKey($number)) { $rows[$number] = @{} }
        $rows[$number]['D'] = '<c r="D' + $number + '"><f>' + (Get-Escaped $formula) + '</f></c>'
    }
    $text = $Decl + '<worksheet xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><sheetData>'
    foreach ($row in ($rows.Keys | Sort-Object)) {
        $text += '<row r="' + $row + '">'
        foreach ($column in ($rows[$row].Keys | Sort-Object { $_.Length }, { $_ })) {
            $text += $rows[$row][$column]
        }
        $text += '</row>'
    }
    return $text + '</sheetData></worksheet>'
}

function New-Relationships($Relationships) {
    $text = $Decl + '<Relationships xmlns="' + $PkgRel + '">'
    foreach ($r in $Relationships) {
        $text += '<Relationship Id="' + $r[0] + '" Type="' + $r[1] + '" Target="' + $r[2] + '"/>'
    }
    return $text + '</Relationships>'
}

$Styles = $Decl + '<styleSheet xmlns="' + $Main + '">' +
    '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>' +
    '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>' +
    '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>' +
    '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>' +
    '<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>' +
    '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>' +
    '</styleSheet>'

# The metadata part: one cell metadata block (a cell's cm="1") whose record names the metadata type
# XLDAPR and its one future-metadata block, whose dynamicArrayProperties sets fDynamic, as Excel
# with dynamic arrays writes it for a dynamic-array formula.
$Metadata = $Decl + '<metadata xmlns="' + $Main + '" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray">' +
    '<metadataTypes count="1"><metadataType name="XLDAPR" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1" cellMeta="1"/></metadataTypes>' +
    '<futureMetadata name="XLDAPR" count="1"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk></futureMetadata>' +
    '<cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata>' +
    '</metadata>'

# Writes a workbook of the worksheets $Sheets (each @(name, worksheet XML)), in order; with
# -DynamicArrays also the metadata part, which the source formulas of New-OracleSheet name.
function Write-Workbook([string]$Path, $Sheets, [switch]$DynamicArrays) {
    $types = $Decl + '<Types xmlns="' + $ContentTypes + '">' +
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
        '<Default Extension="xml" ContentType="application/xml"/>' +
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>' +
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
    if ($DynamicArrays) {
        $types += '<Override PartName="/xl/metadata.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"/>'
    }
    $rels = New-Object System.Collections.ArrayList
    $parts = New-Object System.Collections.ArrayList
    $entries = ''
    $index = 0
    foreach ($sheet in $Sheets) {
        $index += 1
        $entries += '<sheet name="' + $sheet[0] + '" sheetId="' + $index + '" r:id="rId' + $index + '"/>'
        [void]$rels.Add(@("rId$index", "$Rel/worksheet", "worksheets/sheet$index.xml"))
        $types += '<Override PartName="/xl/worksheets/sheet' + $index + '.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        [void]$parts.Add(@("xl/worksheets/sheet$index.xml", $sheet[1]))
    }
    [void]$rels.Add(@(('rId' + ($index + 1)), "$Rel/styles", 'styles.xml'))
    if ($DynamicArrays) {
        [void]$rels.Add(@(('rId' + ($index + 2)), "$Rel/sheetMetadata", 'metadata.xml'))
    }
    $types += '</Types>'
    $workbook = $Decl + '<workbook xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><bookViews><workbookView/></bookViews>' +
        '<sheets>' + $entries + '</sheets><calcPr fullCalcOnLoad="1"/></workbook>'
    $all = New-Object System.Collections.ArrayList
    [void]$all.Add(@('[Content_Types].xml', $types))
    [void]$all.Add(@('_rels/.rels', (New-Relationships @(, @('rId1', "$Rel/officeDocument", 'xl/workbook.xml')))))
    [void]$all.Add(@('xl/workbook.xml', $workbook))
    [void]$all.Add(@('xl/_rels/workbook.xml.rels', (New-Relationships $rels)))
    [void]$all.Add(@('xl/styles.xml', $Styles))
    if ($DynamicArrays) { [void]$all.Add(@('xl/metadata.xml', $Metadata)) }
    foreach ($part in $parts) { [void]$all.Add($part) }
    Write-Package $Path $all
}

# --- cfunc_m1_2/oracle.xlsx ---------------------------------------------------------------------

# --- f1_prefix/oracle.xlsx ----------------------------------------------------------------------

# Sheet Data: the constant cells the formulas read (A1:A3 the numbers 1, 5 and 3; B1:B3 the texts
# x, y and x; C1 #N/A; E1:E3 the numbers 1, 2 and 3).
# Sheet prefix_metadata: one row per named function F1 catalogues, in name order, then each later
# package's section (L1: OFFSET and ISOMITTED), each with one
# call of it over literals and Data's cells: in column A the call written with the prefix _xlfn.,
# in column B the same call without a prefix.
$prefixInputs = @(
    (New-Number 'A1' '1'), (New-Number 'A2' '5'), (New-Number 'A3' '3'),
    (New-Text 'B1' 'x'), (New-Text 'B2' 'y'), (New-Text 'B3' 'x'),
    (New-Error 'C1' '#N/A'),
    (New-Number 'E1' '1'), (New-Number 'E2' '2'), (New-Number 'E3' '3')
)
# calls.txt keeps each package's calls in a section of its own, headed by a line starting with #.
$prefixCalls = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'f1_prefix/calls.txt') |
    Where-Object { $_ -ne '' -and -not $_.StartsWith('#') }

# The sheet prefix_metadata: row n holds call n of $Calls, with the prefix _xlfn. in column A and
# without it in column B.
function New-PrefixSheet($Calls) {
    $text = $Decl + '<worksheet xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><sheetData>'
    $number = 0
    foreach ($call in $Calls) {
        $number += 1
        $text += '<row r="' + $number + '"><c r="A' + $number + '"><f>_xlfn.' + (Get-Escaped $call) + '</f></c>' +
            '<c r="B' + $number + '"><f>' + (Get-Escaped $call) + '</f></c></row>'
    }
    return $text + '</sheetData></worksheet>'
}

Write-Workbook (Join-Path $OutDir 'f1_prefix/oracle.xlsx') @(
    @('Data', (New-OracleSheet $prefixInputs @())),
    @('prefix_metadata', (New-PrefixSheet $prefixCalls))
)
