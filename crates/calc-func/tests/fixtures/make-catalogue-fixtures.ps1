# The generator of the catalogue oracle under crates/calc-func/tests/fixtures/catalogue/.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-catalogue-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/catalogue/oracle.xlsx. Run with this directory as -OutDir it rewrites the
# committed fixture; the acceptance case dc_xrxb__prefix_metadata_matches_excel runs it into a
# fresh directory on Windows and compares its output with the committed workbook byte for byte.
#
# catalogue/oracle.xlsx is the one oracle workbook of the functions the catalogue adds from the
# bulk port on (F1 dc-8dq first; F2 and the later packages append their own sections), which the
# Excel capture reads (`cargo xtask capture --fixture
# crates/calc-func/tests/fixtures/catalogue/oracle.xlsx --case catalogue__oracle --out
# crates/calc-func/tests/captures/catalogue/oracle.capture.json --work-dir <dir>`, keeping
# <dir>/out/oracle.xlsx as captures/catalogue/oracle.saved.xlsx). The functions catalogued before
# the bulk port keep their rows in dc_xrxb/oracle.xlsx (make-fixtures.ps1).
#
# Its sheets:
# - Data: the constant cells the formulas read (A1:A3 the numbers 1, 5 and 3; B1:B3 the texts x,
#   y and x; C1 #N/A; E1:E3 the numbers 1, 2 and 3).
# - prefix_metadata: one row per catalogued function, in the layout of dc_xrxb/oracle.xlsx: in
#   column A its call written with the prefix _xlfn., in column B the same call without one. The
#   rows are the lines of the package sections' prefix files, in the order of $PrefixSections,
#   each file's lines in order; the case reads the function from the text before the call's '('.
# - one sheet per entry of $FormulaSheets: its file's lines, one formula per row of column D
#   from row 1 (written without '='), for rows that are not one call per function (operators,
#   edge cases). The corpus comparison judges them with every other cell.
#
# A package adds its own section: a line adding its prefix file to $PrefixSections, and lines
# adding its sheets to $FormulaSheets, each with its own file, after the sections above it; it does
# not edit another section's files, so the rows above its own keep their places.
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
# column order. $Before is written before sheetData (sheetViews, cols), as the schema orders it.
function New-OracleSheet($Inputs, $Formulas, $Sources = @(), [string]$Before = '') {
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
    $text = $Decl + '<worksheet xmlns="' + $Main + '" xmlns:r="' + $Rel + '">' + $Before + '<sheetData>'
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

# The styles part: the one font, fills and borders, and the cell formats: the default one (index 0)
# and those a section adds with Add-CellFormat, after the custom number formats Add-NumberFormat
# declares.
$NumberFormats = New-Object System.Collections.ArrayList
$CellFormats = New-Object System.Collections.ArrayList
[void]$CellFormats.Add('<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>')

# Declares the custom number format $Code (from ID 164 on) and returns its ID.
function Add-NumberFormat([string]$Code) {
    $id = 164 + $NumberFormats.Count
    [void]$NumberFormats.Add('<numFmt numFmtId="' + $id + '" formatCode="' + (Get-Escaped $Code).Replace('"', '&quot;') + '"/>')
    return $id
}

# Adds a cell format of number format $NumFmtId with the attributes $Attributes and the child
# elements $Children (alignment, protection) and returns its index, which a cell's s names.
function Add-CellFormat([int]$NumFmtId, [string]$Attributes = '', [string]$Children = '') {
    $xf = '<xf numFmtId="' + $NumFmtId + '" fontId="0" fillId="0" borderId="0" xfId="0"'
    if ($NumFmtId -ne 0) { $xf += ' applyNumberFormat="1"' }
    $xf += $Attributes
    if ($Children -eq '') { $xf += '/>' } else { $xf += '>' + $Children + '</xf>' }
    [void]$CellFormats.Add($xf)
    return $CellFormats.Count - 1
}

function Get-Styles {
    $numFmts = ''
    if ($NumberFormats.Count -gt 0) { $numFmts = '<numFmts count="' + $NumberFormats.Count + '">' + ($NumberFormats -join '') + '</numFmts>' }
    return $Decl + '<styleSheet xmlns="' + $Main + '">' + $numFmts +
        '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>' +
        '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>' +
        '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>' +
        '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>' +
        '<cellXfs count="' + $CellFormats.Count + '">' + ($CellFormats -join '') + '</cellXfs>' +
        '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>' +
        '</styleSheet>'
}

# A constant cell of $Cell (as New-Number and the others make it) with the cell format $Style.
function Set-Style($Cell, [int]$Style) {
    return @($Cell[0], ($Cell[1] -replace '^<c r="([A-Z]+[0-9]+)"', ('<c r="$1" s="' + $Style + '"')))
}

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
    [void]$all.Add(@('xl/styles.xml', (Get-Styles)))
    if ($DynamicArrays) { [void]$all.Add(@('xl/metadata.xml', $Metadata)) }
    foreach ($part in $parts) { [void]$all.Add($part) }
    Write-Package $Path $all
}

# --- cfunc_m1_2/oracle.xlsx ---------------------------------------------------------------------

# --- catalogue/oracle.xlsx ----------------------------------------------------------------------

# The sections, in order. Each package appends its own lines here and nowhere else.
$PrefixSections = @()
$FormulaSheets = @()
# F1 dc-8dq: math, statistics and engineering; the ^ and POWER edge cases.
$PrefixSections += 'prefix-f1.txt'
$FormulaSheets += , @('f1_power', 'sheet-f1_power.txt')
# L1 dc-mzk: OFFSET and ISOMITTED.
$PrefixSections += 'prefix-l1.txt'
# F2 dc-ip6: text, date and time, information, logical, lookup, database and web text.
$PrefixSections += 'prefix-f2.txt'
$FormulaSheets += , @('f2_rows', 'sheet-f2_rows.txt')
# F3 dc-728: financial, dynamic-array and LAMBDA-helper functions.
$PrefixSections += 'prefix-f3.txt'
$FormulaSheets += , @('f3_financial', 'sheet-f3_financial.txt')
$FormulaSheets += , @('f3_arrays', 'sheet-f3_arrays.txt')
# The function exactness lane (dc-j9k, dc-6qki, dc-uar): readings of Excel's exact results.
$FormulaSheets += , @('dc6qki_arith', 'sheet-dc6qki_arith.txt')
$FormulaSheets += , @('dcj9k_regression', 'sheet-dcj9k_regression.txt')
$FormulaSheets += , @('dcj9k_closed_form', 'sheet-dcj9k_closed_form.txt')
$FormulaSheets += , @('dcuar_text', 'sheet-dcuar_text.txt')
$FormulaSheets += , @('dcj9k_annuity', 'sheet-dcj9k_annuity.txt')
$FormulaSheets += , @('dcj9k_statistics', 'sheet-dcj9k_statistics.txt')
$FormulaSheets += , @('dcj9k_math', 'sheet-dcj9k_math.txt')
$FormulaSheets += , @('dcj9k_linest_stats', 'sheet-dcj9k_linest_stats.txt')
# dc-b1l: the oracle corpus's refused cells (FIELDVALUE first).
$PrefixSections += 'prefix-b1l.txt'
$FormulaSheets += , @('dcb1l_language', 'sheet-dcb1l_language.txt')
# dc-zsn.1: text comparison and sorting beyond ASCII.
$FormulaSheets += , @('dczsn1_collation', 'sheet-dczsn1_collation.txt')
# dc-fl2: the text functions over surrogates and the 32,767-unit limit.
$FormulaSheets += , @('dcfl2_text', 'sheet-dcfl2_text.txt')
# dc-zsn.2: text comparison over every block (symbols, private use, the other scripts,
# combining marks the walk decides, iteration marks, decompositions).
$FormulaSheets += , @('dczsn2_collation', 'sheet-dczsn2_collation.txt')
# dc-zsn.3: ASC, DBCS and the byte functions under a language that is not double-byte.
$FormulaSheets += , @('dczsn3_bytes', 'sheet-dczsn3_bytes.txt')
# dc-h9u: the functions OxFunc has and calc-func lacked (PERCENTOF, TRIMRANGE first).
$PrefixSections += 'prefix-h9u.txt'
$FormulaSheets += , @('dch9u_functions', 'sheet-dch9u.txt')
# dc-96l.3: the clean-room replacements (the Bessel functions, the discrete distributions, the x87 chains).
$FormulaSheets += , @('dc96l3_bessel', 'sheet-dc96l3_bessel.txt')
$FormulaSheets += , @('dc96l3_discrete', 'sheet-dc96l3_discrete.txt')
$FormulaSheets += , @('dc96l3_x87', 'sheet-dc96l3_x87.txt')

$prefixInputs = @(
    (New-Number 'A1' '1'), (New-Number 'A2' '5'), (New-Number 'A3' '3'),
    (New-Text 'B1' 'x'), (New-Text 'B2' 'y'), (New-Text 'B3' 'x'),
    (New-Error 'C1' '#N/A'),
    (New-Number 'E1' '1'), (New-Number 'E2' '2'), (New-Number 'E3' '3')
)

# The lines of a section file beside this script, without empty lines and lines starting with #
# (comments).
function Get-SectionLines([string]$Name) {
    return @(Get-Content -LiteralPath (Join-Path (Join-Path $PSScriptRoot 'catalogue') $Name) |
        Where-Object { $_ -ne '' -and -not $_.StartsWith('#') })
}

$prefixCalls = @()
foreach ($section in $PrefixSections) { $prefixCalls += Get-SectionLines $section }

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

$sheets = @(
    @('Data', (New-OracleSheet $prefixInputs @())),
    @('prefix_metadata', (New-PrefixSheet $prefixCalls))
)
foreach ($sheet in $FormulaSheets) {
    $sheets += , @($sheet[0], (New-OracleSheet @() (Get-SectionLines $sheet[1])))
}

# dc-uar: CELL's info types and INFO, on the sheet dcuar_cell. Its constant cells carry formats:
# column A the prefix cases (alignments, kinds of value), column B the number 1.5 under each code
# of formats-dcuar_cell.txt (each line `code:<custom code>` or `id:<built-in ID>`), column C the
# protection cases; columns E to K the column widths; the sheet's window is saved scrolled to C5.
# Its formulas are sheet-dcuar_cell.txt's lines, then CELL("format"), CELL("color") and
# CELL("parentheses") of each cell of column B.
function Get-Aligned([string]$Horizontal) {
    return Add-CellFormat 0 ' applyAlignment="1"' ('<alignment horizontal="' + $Horizontal + '"/>')
}
$cellInputs = @()
$cellInputs += , (New-Text 'A1' 'a')
$cellInputs += , (Set-Style (New-Text 'A2' 'a') (Get-Aligned 'left'))
$cellInputs += , (Set-Style (New-Text 'A3' 'a') (Get-Aligned 'right'))
$cellInputs += , (Set-Style (New-Text 'A4' 'a') (Get-Aligned 'center'))
$cellInputs += , (Set-Style (New-Text 'A5' 'a') (Get-Aligned 'fill'))
$cellInputs += , (Set-Style (New-Text 'A6' 'a') (Get-Aligned 'justify'))
$cellInputs += , (Set-Style (New-Text 'A7' 'a') (Get-Aligned 'centerContinuous'))
$cellInputs += , (Set-Style (New-Text 'A8' 'a') (Get-Aligned 'distributed'))
$cellInputs += , (Set-Style (New-Number 'A9' '5') (Get-Aligned 'right'))
$cellInputs += , (Set-Style (New-Logical 'A10' $true) (Get-Aligned 'left'))
$cellInputs += , (Set-Style (New-Error 'A11' '#N/A') (Get-Aligned 'center'))
$cellInputs += , @('A12', ('<c r="A12" s="' + (Get-Aligned 'left') + '"/>'))
$cellInputs += , (Set-Style (New-Text 'A13' '5') (Add-CellFormat 0 ' quotePrefix="1"'))
$cellInputs += , (Set-Style (New-Text 'A14' 'a') (Add-CellFormat 49))
$cellInputs += , (New-Text 'A15' ' ')
$cellInputs += , (New-Number 'A16' '7')
$cellInputs += , (Set-Style (New-Text 'A17' 'a') (Add-CellFormat 0 ' applyAlignment="1"' '<alignment horizontal="left" indent="2"/>'))
$cellInputs += , (New-Number 'C1' '1')
$cellInputs += , (Set-Style (New-Number 'C2' '1') (Add-CellFormat 0 ' applyProtection="1"' '<protection locked="0"/>'))
$cellInputs += , (Set-Style (New-Number 'C3' '1') (Add-CellFormat 0 ' applyProtection="1"' '<protection hidden="1"/>'))
$cellInputs += , (Set-Style (New-Number 'C4' '1') (Add-CellFormat 0 ' applyProtection="1"' '<protection locked="0" hidden="1"/>'))
$cellInputs += , (Set-Style (New-Text 'C5' 'a') (Add-CellFormat 0 ' applyProtection="1"' '<protection locked="1"/>'))
$cellCodes = Get-SectionLines 'formats-dcuar_cell.txt'
$cellFormulas = @(Get-SectionLines 'sheet-dcuar_cell.txt')
$number = 0
foreach ($code in $cellCodes) {
    $number += 1
    if ($code.StartsWith('id:')) { $id = [int]$code.Substring(3) } else { $id = Add-NumberFormat $code.Substring(5) }
    $cellInputs += , (Set-Style (New-Number ('B' + $number) '1.5') (Add-CellFormat $id))
    foreach ($kind in @('format', 'color', 'parentheses')) { $cellFormulas += 'CELL("' + $kind + '",B' + $number + ')' }
}
$cellBefore = '<sheetViews><sheetView topLeftCell="C5" workbookViewId="0"/></sheetViews>' +
    '<cols><col min="6" max="6" width="20.7109375" customWidth="1"/><col min="7" max="7" width="0" hidden="1" customWidth="1"/>' +
    '<col min="8" max="8" width="9.140625"/><col min="9" max="9" width="12.7109375" customWidth="1"/>' +
    '<col min="10" max="10" width="0.7109375" customWidth="1"/><col min="11" max="11" width="255.7109375" customWidth="1"/></cols>'
$sheets += , @('dcuar_cell', (New-OracleSheet $cellInputs $cellFormulas @() $cellBefore))
Write-Workbook (Join-Path $OutDir 'catalogue/oracle.xlsx') $sheets
