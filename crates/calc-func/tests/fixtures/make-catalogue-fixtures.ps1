# The generator of the catalogue oracle under crates/calc-func/tests/fixtures/catalogue/.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-catalogue-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/catalogue/oracle.xlsx and separate numeric research question workbooks.
# Run with this directory as -OutDir it rewrites the
# committed fixture; the acceptance case dc_xrxb__prefix_metadata_matches_excel runs it into a
# fresh directory on Windows and compares its output with the committed workbook byte for byte.
#
# catalogue/oracle.xlsx is the one oracle workbook of the functions the catalogue adds from the
# bulk port on (F1 dc-8dq first; F2 and the later packages append their own sections). It is
# captured per sheet: crates/calc-func/tests/captures/catalogue/sections/<sheet>.capture.json is
# Excel's capture of one sheet, keyed by what decides its results (the sheet and the sheets it
# reads; calc_verdict::workbook_sections), so a sheet added or changed leaves every other sheet's
# capture valid. A sheet with no capture of its current content is awaiting capture: counted
# apart in `cargo xtask corpus --oracle`, failing no test. `cargo xtask capture --pending` takes
# every such sheet, each in its own Excel session (prefix_metadata also keeps the worksheet Excel
# saved, sections/prefix_metadata.saved.xml). The functions catalogued before the bulk port keep
# their rows in dc_xrxb/oracle.xlsx (make-fixtures.ps1).
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
    [Parameter(Mandatory = $true)][string]$OutDir,
    [string]$Fn2IndexShapes,
    [string]$QuestionSectionFile = '',
    [string]$QuestionCase = '',
    [string]$QuestionSetupFile = ''
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
function New-OracleSheet($Inputs, $Formulas, $Sources = @(), [string]$Before = '', [string]$FormulaColumn = 'D') {
    $rows = @{}
    foreach ($cell in $Inputs) {
        $place = Split-Ref $cell[0]
        if ($place[1] -eq $FormulaColumn) { throw "input cell $($cell[0]) is in the formula column D" }
        if (-not $rows.ContainsKey($place[0])) { $rows[$place[0]] = @{} }
        $rows[$place[0]][$place[1]] = $cell[1]
    }
    foreach ($source in $Sources) {
        $place = Split-Ref $source[0]
        if ($place[1] -eq $FormulaColumn) { throw "source cell $($source[0]) is in the formula column D" }
        if (-not $rows.ContainsKey($place[0])) { $rows[$place[0]] = @{} }
        if ($rows[$place[0]].ContainsKey($place[1])) { throw "cell $($source[0]) is written twice" }
        $rows[$place[0]][$place[1]] = '<c r="' + $source[0] + '" cm="1"><f t="array" ref="' + $source[0] + '">' + (Get-Escaped $source[1]) + '</f></c>'
    }
    $number = 0
    foreach ($formula in $Formulas) {
        $number += 1
        if (-not $rows.ContainsKey($number)) { $rows[$number] = @{} }
        $rows[$number][$FormulaColumn] = '<c r="' + $FormulaColumn + $number + '"><f>' + (Get-Escaped $formula) + '</f></c>'
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
# Question families may request the 1904 date system or workbook-scoped formula names.
# Omitting both keeps the original package bytes unchanged.
function Write-Workbook([string]$Path, $Sheets, [switch]$DynamicArrays, [switch]$Date1904, $Names = @()) {
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
    $properties = ''
    if ($Date1904) { $properties = '<workbookPr date1904="1"/>' }
    $definedNames = ''
    if ($Names.Count -gt 0) {
        $definedNames = '<definedNames>'
        foreach ($name in $Names) {
            $definedNames += '<definedName name="' + (Get-Escaped $name[0]) + '">' + (Get-Escaped $name[1]) + '</definedName>'
        }
        $definedNames += '</definedNames>'
    }
    $workbook = $Decl + '<workbook xmlns="' + $Main + '" xmlns:r="' + $Rel + '">' + $properties + '<bookViews><workbookView/></bookViews>' +
        '<sheets>' + $entries + '</sheets>' + $definedNames + '<calcPr fullCalcOnLoad="1"/></workbook>'
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

# Corpus-seeded questions use the same package writer in a separate new workbook.
# This mode returns before reading or writing any of the existing oracle cases.
if ($QuestionSectionFile -ne '' -or $QuestionCase -ne '') {
    if ($QuestionSectionFile -eq '' -or $QuestionCase -cnotmatch '^[a-z][a-z0-9_]{0,30}$') {
        throw 'Questions require -QuestionSectionFile and a lowercase -QuestionCase (at most 31 characters).'
    }
    $questionPath = Join-Path $OutDir ($QuestionCase + '/oracle.xlsx')
    if (Test-Path -LiteralPath $questionPath) { throw ('Question workbook already exists: ' + $questionPath) }
    $questionFormulas = @([System.IO.File]::ReadAllLines($QuestionSectionFile) | Where-Object {
        $_.Trim() -ne '' -and -not $_.TrimStart().StartsWith('#')
    })
    if ($questionFormulas.Count -eq 0) { throw 'Question section contains no formulas.' }
    $questionInputs = @(); $questionColumn = 'D'
    if ($QuestionSetupFile -ne '') {
        $setup = Get-Content -LiteralPath $QuestionSetupFile -Raw -Encoding utf8 | ConvertFrom-Json
        $questionColumn = 'F'
        foreach ($entry in $setup.PSObject.Properties) {
            $at = [string]$entry.Name; $value = $entry.Value
            if ($value.PSObject.Properties['number_bits']) {
                $number = [BitConverter]::Int64BitsToDouble([Convert]::ToInt64([string]$value.number_bits,16))
                if ([double]::IsNaN($number) -or [double]::IsInfinity($number)) { throw 'Question input must be finite' }
                $questionInputs += ,(New-Number $at ($number.ToString('R',[Globalization.CultureInfo]::InvariantCulture)))
            } elseif ($value.PSObject.Properties['formula']) {
                $questionInputs += ,@($at, ('<c r="' + $at + '"><f>' + (Get-Escaped ([string]$value.formula).TrimStart('=')) + '</f></c>'))
            } else { throw ('Unknown authored setup cell ' + $at) }
        }
    }
    $questionSheets = @(, @($QuestionCase, (New-OracleSheet $questionInputs $questionFormulas -FormulaColumn $questionColumn)))
    Write-Workbook $questionPath $questionSheets
    return
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
$FormulaSheets += , @('dcj9k_r16', 'sheet-dcj9k_r16.txt')
$FormulaSheets += , @('fn1_collation', 'sheet-fn1_collation.txt')

$prefixInputs = @(
    (New-Number 'A1' '1'), (New-Number 'A2' '5'), (New-Number 'A3' '3'),
    (New-Text 'B1' 'x'), (New-Text 'B2' 'y'), (New-Text 'B3' 'x'),
    (New-Error 'C1' '#N/A'),
    (New-Number 'E1' '1'), (New-Number 'E2' '2'), (New-Number 'E3' '3')
)

# The lines of a section file beside this script, without empty lines and lines starting with #
# (comments).
function Get-SectionLines([string]$Name, [string]$Case = 'catalogue') {
    return @(Get-Content -LiteralPath (Join-Path (Join-Path $PSScriptRoot $Case) $Name) -Encoding UTF8 |
        Where-Object { $_ -ne '' -and -not $_.StartsWith('#') })
}

# A section line may carry typed inputs (dc-jnt.5): the formula, then up to six tab-separated
# cells, written as constants in columns A, B, C, E, F and G of the row the formula takes (column D),
# so the formula reads inputs that are exact. {A}, {B}, {C}, {E}, {F} and {G} in the formula stand
# for this row's cells.
# A cell is n:<16 hex digits> (a finite binary64 number, written with 17 significant digits),
# s:<text>, b:TRUE or b:FALSE, e:<error literal>, f:<formula> (a formula cell; text holding control
# characters, which XML 1.0 cannot carry, is built as "a"&CHAR(12)&"b"), or - (the cell stays empty). A line without a tab is
# a plain formula, as before.
$InputColumns = @('A', 'B', 'C', 'E', 'F', 'G')
function New-InputCell([string]$Ref, [string]$Token) {
    if ($Token -eq '-') { return $null }
    if ($Token.Length -lt 2 -or $Token[1] -ne ':') { throw "input cell $Ref has no kind: $Token" }
    $body = $Token.Substring(2)
    switch ($Token[0]) {
        'n' {
            if ($body -cnotmatch '^[0-9a-f]{16}$') { throw "input cell $Ref is not 16 hex digits: $Token" }
            $number = [BitConverter]::Int64BitsToDouble([Convert]::ToInt64($body, 16))
            if ([double]::IsNaN($number) -or [double]::IsInfinity($number)) { throw "input cell $Ref is not finite: $Token" }
            return (New-Number $Ref ($number.ToString('G17', [Globalization.CultureInfo]::InvariantCulture)))
        }
        's' {
            if ($body -ne $body.Trim()) {
                return @($Ref, ('<c r="' + $Ref + '" t="inlineStr"><is><t xml:space="preserve">' + (Get-Escaped $body) + '</t></is></c>'))
            }
            return (New-Text $Ref $body)
        }
        'b' {
            if ($body -ne 'TRUE' -and $body -ne 'FALSE') { throw "input cell $Ref is not TRUE or FALSE: $Token" }
            return (New-Logical $Ref ($body -eq 'TRUE'))
        }
        'e' { return (New-Error $Ref $body) }
        'f' { return @($Ref, ('<c r="' + $Ref + '"><f>' + (Get-Escaped $body) + '</f></c>')) }
        default { throw "input cell $Ref has an unknown kind: $Token" }
    }
}

# The formulas and the input cells of a section's lines (see New-InputCell).
function Expand-SectionLines($Lines) {
    $formulas = @(); $inputs = @(); $number = 0
    foreach ($line in $Lines) {
        $number += 1
        $fields = $line.Split([char]9)
        if ($fields.Count -eq 1) { $formulas += $line; continue }
        if ($fields.Count -gt 7) { throw "line $number has more than six input cells" }
        $text = $fields[0]
        foreach ($column in $InputColumns) { $text = $text.Replace('{' + $column + '}', $column + $number) }
        $formulas += $text
        for ($k = 1; $k -lt $fields.Count; $k++) {
            $cell = New-InputCell ($InputColumns[$k - 1] + $number) $fields[$k]
            if ($null -ne $cell) { $inputs += , $cell }
        }
    }
    return @{ Formulas = $formulas; Inputs = $inputs }
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

# Research questions get a separate workbook. Adding a sheet to the catalogue changes
# SHEETS/INFO context and the position of dcuar_cell, invalidating existing captures.
$questions = @(, @('dcj9k_r21', (New-OracleSheet @() (Get-SectionLines 'sheet-dcj9k_r21.txt' 'dcj9k_r21'))))
Write-Workbook (Join-Path $OutDir 'dcj9k_r21/oracle.xlsx') $questions

$r22Questions = @(, @('dcj9k_r22', (New-OracleSheet @() (Get-SectionLines 'sheet-dcj9k_r22.txt' 'dcj9k_r22'))))
Write-Workbook (Join-Path $OutDir 'dcj9k_r22/oracle.xlsx') $r22Questions

# dc-j9k R38: measured ETS questions use the same generator and a separate
# numeric workbook, preserving every existing SHEETS/INFO capture key.
$r38Ets = @(, @('dcj9k_r38_ets', (New-OracleSheet @() (Get-SectionLines 'sheet-dcj9k_r38_ets.txt' 'dcj9k_r38_ets'))))
$r38Ets += , @('prefix_metadata', (New-PrefixSheet (Get-SectionLines 'prefix-dcj9k_r38_ets.txt' 'dcj9k_r38_ets')))
Write-Workbook (Join-Path $OutDir 'dcj9k_r38_ets/oracle.xlsx') $r38Ets

# R39 nonseasonal fits and the remaining short-initialisation/detection cases.
$r39Ets = @(, @('dcj9k_r39_ets', (New-OracleSheet @() (Get-SectionLines 'sheet-dcj9k_r39_ets.txt' 'dcj9k_r39_ets'))))
Write-Workbook (Join-Path $OutDir 'dcj9k_r39_ets/oracle.xlsx') $r39Ets

# R39 exact seasonal affine histories; the captured nonseasonal oracle is unchanged.
$r39Seasonal = @(, @('dcj9k_r39_seasonal', (New-OracleSheet @() (Get-SectionLines 'sheet-dcj9k_r39_seasonal.txt' 'dcj9k_r39_seasonal'))))
Write-Workbook (Join-Path $OutDir 'dcj9k_r39_seasonal/oracle.xlsx') $r39Seasonal

# dc-b1l.6: every fn2 caller has a blank source sheet and a separate observer sheet.
# Stored text comes from calc-doc's existing file_form serializer; the packet's authored
# spelling, explicit form and caller remain alongside it in this section's layout.
$fn2Sheets = @(, @('Data', (New-OracleSheet $prefixInputs @())))
$fn2Path = Join-Path $PSScriptRoot 'fn2_questions/oracle.entries.json'
$fn2 = Get-Content -LiteralPath $fn2Path -Raw -Encoding UTF8 | ConvertFrom-Json
$indexShapes = @{}
$fn2IndexSheets = @()
if ($Fn2IndexShapes) {
    $shapeLayout = Get-Content -LiteralPath $Fn2IndexShapes -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($shape in $shapeLayout.questions) {
        if ($indexShapes.ContainsKey($shape.question_id)) { throw 'Duplicate INDEX shape question' }
        if ($shape.rows -lt 1 -or $shape.columns -lt 1 -or $shape.rows * $shape.columns -gt 10000) { throw 'INDEX shape outside the bounded probe layout' }
        $indexShapes[$shape.question_id] = $shape
    }
    if ($indexShapes.Count -ne @($fn2.questions).Count) { throw 'INDEX shapes do not cover the authored questions' }
    $fn2IndexSheets = @(, @('Data', (New-OracleSheet $prefixInputs @())))
}
foreach ($question in $fn2.questions) {
    if ($question.formula_form -eq 'Dynamic') {
        $sources = , @($question.source_cell, $question.stored_formula_text)
        $sourceXml = New-OracleSheet @() @() $sources
    } elseif ($question.formula_form -eq 'Legacy') {
        $cellXml = '<c r="' + $question.source_cell + '"><f>' + (Get-Escaped $question.stored_formula_text) + '</f></c>'
        $inputs = , @($question.source_cell, $cellXml)
        $sourceXml = New-OracleSheet $inputs @()
    } else { throw "Unknown fn2 formula form: $($question.formula_form)" }
    $fn2Sheets += , @($question.source_sheet, $sourceXml)
    $fn2Sheets += , @($question.observer_sheet, (New-OracleSheet @() $question.observer_stored_formulas))
    if ($Fn2IndexShapes) {
        if (-not $indexShapes.ContainsKey($question.question_id)) { throw 'INDEX shape is missing an authored question' }
        $shape = $indexShapes[$question.question_id]
        $observers = @($question.observer_stored_formulas)
        for ($row = 1; $row -le $shape.rows; $row++) {
            for ($column = 1; $column -le $shape.columns; $column++) {
                $observers += "INDEX(_xlfn.ANCHORARRAY('$($question.source_sheet)'!$($question.source_cell)),$row,$column)"
            }
        }
        $fn2IndexSheets += , @($question.source_sheet, $sourceXml)
        $fn2IndexSheets += , @($question.observer_sheet, (New-OracleSheet @() $observers))
    }
}

Write-Workbook (Join-Path $OutDir 'fn2_questions/oracle.xlsx') $fn2Sheets -DynamicArrays
if ($Fn2IndexShapes) { Write-Workbook (Join-Path $OutDir 'fn2_questions_index/oracle.xlsx') $fn2IndexSheets -DynamicArrays }

# fn3 questions stay in a new workbook; captured workbook sheet lists remain unchanged.
$fn3Sheets = @(, @('fn3_collation', (New-OracleSheet @() (Get-SectionLines 'sheet-fn3_collation.txt' 'fn3_questions'))))
$fn3Sheets += , @('fn3_text', (New-OracleSheet @() (Get-SectionLines 'sheet-fn3_text.txt' 'fn3_questions')))
Write-Workbook (Join-Path $OutDir 'fn3_questions/oracle.xlsx') $fn3Sheets

# dc-zsn.2.4: separate unanswered SORT questions preserve all captured sheet lists.
$fn10SortSheets = @(, @('fn10_sort_cycle', (New-OracleSheet @() (Get-SectionLines 'sheet-fn10_sort_cycle.txt' 'fn10_sort_cycle'))))
Write-Workbook (Join-Path $OutDir 'fn10_sort_cycle/oracle.xlsx') $fn10SortSheets

# dc-jnt (test migration of the legacy suites): the legacy OxFunc Excel readings, one sheet per batch,
# in a workbook of their own (catalogue sheets cannot be added: every added sheet changes the context
# of prefix_metadata and dcuar_cell, whose captures must stay valid). A line of a section file is a
# formula and the typed inputs it reads (Expand-SectionLines); the answers are the legacy readings,
# imported into the section captures by tools/excel-service/jnt_import.py. Each batch appends its
# sheet to $JntSheets, whose section files are fixtures/jnt/sheet-<name>.txt.
$JntSheets = @()
$JntSheets += 'jnt_w111_elementary'
$JntSheets += 'jnt_w111_rounding'
$JntSheets += 'jnt_w111_text_dates'
$JntSheets += 'jnt_dates_random'
$JntSheets += 'jnt_operators'
$JntSheets += 'jnt_parity_complex'
$JntSheets += 'jnt_logical_info'
$JntSheets += 'jnt_lookup'
$JntSheets += 'jnt_text_formats'
$JntSheets += 'jnt_aggregate_arrays'
$JntSheets += 'jnt_evaluator_callables'
$JntSheets += 'jnt_parse_bind'
$jnt = @()
foreach ($name in $JntSheets) {
    $expanded = Expand-SectionLines (Get-SectionLines ('sheet-' + $name + '.txt') 'jnt')
    $jnt += , @($name, (New-OracleSheet $expanded.Inputs $expanded.Formulas))
}
Write-Workbook (Join-Path $OutDir 'jnt/oracle.xlsx') $jnt
