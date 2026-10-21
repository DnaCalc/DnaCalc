# The generator of the workbook fixtures under crates/calc-fml/tests/fixtures/.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/dc_ibp2/oracle.xlsx and <directory>/dc_l10u/oracle.xlsx. Run with this
# directory as -OutDir it rewrites the committed fixtures; the acceptance cases
# dc_ibp2__error_fallback and dc_l10u__reference_results_as_values run it into a fresh directory on
# Windows and compare its output with the committed workbooks byte for byte.
#
# dc_ibp2/oracle.xlsx is the workbook the Excel capture of bead dc-ibp2 reads
# (`cargo xtask capture --fixture crates/calc-fml/tests/fixtures/dc_ibp2/oracle.xlsx ...`). It has
# one sheet, error_fallback: the constant cells the case's oracle rows read, in columns A to C and
# F, as the case's reader holds them; in column F the source formulas, each written as a
# dynamic-array formula over its one cell (cell metadata naming the dynamic-array metadata XLDAPR
# of the metadata part), whose errors some oracle rows catch; and in column D one formula per
# oracle row, from row 1, in the order the case runs its rows. The case checks that the sheet holds
# exactly those formulas and cells. No formula cell carries a cache: Excel calculates them.
#
# The package is a minimal SpreadsheetML package written as text: every part is UTF-8 XML, stored
# (ZIP method 0, no compression) with the timestamp 1980-01-01 00:00 and no extra fields, by the
# ZIP writer below, so the bytes depend on nothing but this script. The script is ASCII and runs
# under Windows PowerShell 5.1.

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

# A formula as the file stores it: a function added to Excel after Excel 2007 carries the prefix
# _xlfn. in the package (FILTER also _xlws.), and Excel reads the name without it as an unknown
# name (#NAME?). IFNA (Excel 2013), the source formulas' FILTER, SEQUENCE and FIELDVALUE, and
# XLOOKUP are the functions of these cells that need it.
function ConvertTo-FileForm([string]$Formula) {
    return $Formula.Replace('IFNA(', '_xlfn.IFNA(').Replace('FILTER(', '_xlfn._xlws.FILTER(').Replace('SEQUENCE(', '_xlfn.SEQUENCE(').Replace('FIELDVALUE(', '_xlfn.FIELDVALUE(').Replace('XLOOKUP(', '_xlfn.XLOOKUP(')
}

# A worksheet part: the constant cells $Inputs (each @(reference, XML), in columns A to C and F),
# the source formulas $Sources (each @(reference, formula as entered), in column F), each written in
# its file form as a dynamic-array formula over its one cell (cm="1", the metadata part's one cell
# metadata block), and the formulas $Formulas, as entered, one per row of column D from row 1,
# each written in its file form. Rows are written in order, each row's cells in column order.
function New-OracleSheet($Inputs, $Sources, $Formulas) {
    $rows = @{}
    $put = {
        param([string]$Ref, [string]$Xml, [string]$Columns)
        $place = Split-Ref $Ref
        if ($Columns.IndexOf($place[1]) -lt 0 -or $place[1].Length -ne 1) { throw "cell $Ref is outside columns $Columns" }
        if (-not $rows.ContainsKey($place[0])) { $rows[$place[0]] = @{} }
        if ($rows[$place[0]].ContainsKey($place[1])) { throw "cell $Ref is written twice" }
        $rows[$place[0]][$place[1]] = $Xml
    }
    foreach ($cell in $Inputs) { & $put $cell[0] $cell[1] 'ABCF' }
    foreach ($source in $Sources) {
        & $put $source[0] ('<c r="' + $source[0] + '" cm="1"><f t="array" ref="' + $source[0] + '">' + (Get-Escaped (ConvertTo-FileForm $source[1])) + '</f></c>') 'F'
    }
    $number = 0
    foreach ($formula in $Formulas) {
        $number += 1
        & $put "D$number" ('<c r="D' + $number + '"><f>' + (Get-Escaped (ConvertTo-FileForm $formula)) + '</f></c>') 'D'
    }
    $text = $Decl + '<worksheet xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><sheetData>'
    foreach ($row in ($rows.Keys | Sort-Object)) {
        $text += '<row r="' + $row + '">'
        foreach ($column in @('A', 'B', 'C', 'D', 'F')) {
            if ($rows[$row].ContainsKey($column)) { $text += $rows[$row][$column] }
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

# Writes a workbook of the worksheets $Sheets (each @(name, worksheet XML)), in order, with the
# metadata part.
function Write-Workbook([string]$Path, $Sheets) {
    $types = $Decl + '<Types xmlns="' + $ContentTypes + '">' +
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
        '<Default Extension="xml" ContentType="application/xml"/>' +
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>' +
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>' +
        '<Override PartName="/xl/metadata.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"/>'
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
    [void]$rels.Add(@(('rId' + ($index + 2)), "$Rel/sheetMetadata", 'metadata.xml'))
    $types += '</Types>'
    $workbook = $Decl + '<workbook xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><bookViews><workbookView/></bookViews>' +
        '<sheets>' + $entries + '</sheets><calcPr fullCalcOnLoad="1"/></workbook>'
    $all = New-Object System.Collections.ArrayList
    [void]$all.Add(@('[Content_Types].xml', $types))
    [void]$all.Add(@('_rels/.rels', (New-Relationships @(, @('rId1', "$Rel/officeDocument", 'xl/workbook.xml')))))
    [void]$all.Add(@('xl/workbook.xml', $workbook))
    [void]$all.Add(@('xl/_rels/workbook.xml.rels', (New-Relationships $rels)))
    [void]$all.Add(@('xl/styles.xml', $Styles))
    [void]$all.Add(@('xl/metadata.xml', $Metadata))
    foreach ($part in $parts) { [void]$all.Add($part) }
    Write-Package $Path $all
}

# --- dc_ibp2/oracle.xlsx ------------------------------------------------------------------------

# Sheet error_fallback: the constant cells of dc_ibp2__error_fallback's reader on that sheet (A4
# blank; A5 empty text held as a constant; A2, A3 and B1 to B5 one error of each kind a cell can
# hold as a constant; F3 the text that blocks F2's spill), the source formulas of column F, whose
# errors the case's reader holds as it assumes Excel gives them (F1: FILTER with no match, #CALC!;
# F2: SEQUENCE(2) blocked by F3, #SPILL!; F4: FIELDVALUE of a number, #FIELD!), and its oracle
# rows' formulas in its order.
$fallbackInputs = @(
    (New-Number 'A1' '5'),
    (New-Error 'A2' '#DIV/0!'),
    (New-Error 'A3' '#N/A'),
    (New-Text 'A5' ''),
    (New-Text 'A6' 'a'),
    (New-Logical 'A7' $true),
    (New-Text 'A8' '2'),
    (New-Error 'B1' '#REF!'),
    (New-Error 'B2' '#VALUE!'),
    (New-Error 'B3' '#NUM!'),
    (New-Error 'B4' '#NAME?'),
    (New-Error 'B5' '#NULL!'),
    (New-Number 'B6' '0'),
    (New-Number 'B7' '2'),
    (New-Text 'F3' 'block')
)
$fallbackSources = @(
    @('F1', 'FILTER(B6:B7,B6:B7>100)'),
    @('F2', 'SEQUENCE(2)'),
    @('F4', 'FIELDVALUE(A1,"x")')
)
$fallbackFormulas = @(
    # The bead's four rows, then an error literal of each kind a cell can hold as a constant, and
    # #GETTING_DATA.
    'IFERROR(1/0,5)', 'IFERROR(2,1/0)', 'IFNA(#N/A,1)', 'IFNA(#DIV/0!,1)',
    'IFERROR(#N/A,"na")', 'IFERROR(#VALUE!,1)', 'IFERROR(#REF!,1)', 'IFERROR(#NAME?,1)',
    'IFERROR(#NUM!,1)', 'IFERROR(#NULL!,1)', 'IFERROR(#DIV/0!,1)', 'IFERROR(#GETTING_DATA,1)',
    'IFNA(#GETTING_DATA,1)', 'IFNA(#VALUE!,1)', 'IFNA(#REF!,"x")', 'IFNA(#NAME?,1)', 'IFNA(#NUM!,1)',
    'IFNA(#NULL!,1)',
    # References to error cells.
    'IFERROR(A2,"caught")', 'IFNA(A2,"caught")', 'IFNA(A3,"caught")', 'IFERROR(A3,"caught")',
    'IFERROR(B1,1)', 'IFERROR(B2,1)', 'IFERROR(B3,1)', 'IFERROR(B4,1)', 'IFERROR(B5,1)',
    'IFNA(B1,1)', 'IFNA(B5,1)',
    # References to cells holding values, a blank cell and empty text among them, and omitted
    # arguments.
    'IFERROR(A1,0)', 'IFNA(A1,0)', 'IFERROR(A6,1)', 'IFERROR(A7,1)', 'IFERROR(A5,"x")',
    'IFERROR(A4,"x")', 'IFNA(A4,"x")', 'IFERROR(#N/A,A4)', 'IFERROR(A4,1)&"|"', 'IFERROR(A4,1)+1',
    'IFERROR(A4,1)=0', 'IFERROR(,1)', 'IFERROR(#N/A,)', 'IFNA(,1)', 'IFERROR(#N/A,)&"|"',
    'IFERROR(,1)&"|"',
    # The fallback: read only on an error, and returned as it is.
    'IFERROR(A2,B7)', 'IFERROR(A1,B7)', 'IFERROR(A2,A3)', 'IFNA(#N/A,#DIV/0!)', 'IFERROR(1/0,1/0)',
    'IFERROR(1,2^0.5)', 'IFERROR(1,A1:A2)', 'IFNA(A1,2^0.5)',
    # Nested inside arithmetic and other calls.
    '1+IFERROR(1/0,5)', 'IFERROR(A2,1)*10', '2*IFNA(A3,4)', '-IFERROR(A2,3)',
    'IFERROR(IFERROR(1/0,#N/A),2)', 'IFNA(IFERROR(A2,#N/A),"x")', 'IFERROR("a"+1,"bad")',
    'IFERROR(A6*2,0)', 'IFERROR(A1/B6,"div")', 'IFERROR(A1/B7,"div")', 'IF(IFERROR(A2,FALSE),1,2)',
    'IFERROR(IF(A1>3,A2,1),"x")', 'SUM(IFERROR(A2,1),A1)', 'IFERROR(SUM(A1:A3),0)',
    'SUM(IFERROR(A8,0))', 'SUM(IF(TRUE,A8))', 'IFERROR(1/0,"a")&"b"',
    # A final sum near zero inside either argument, and two values as they are.
    'IFERROR(0.5-0.4-0.1,1)', 'IFERROR(#N/A,0.5-0.4-0.1)', 'IFERROR(TRUE,1)', 'IFERROR("",1)',
    # The errors of the source formulas of column F.
    'IFERROR(F1,1)', 'IFNA(F1,1)', 'IFERROR(F2,1)', 'IFNA(F2,1)', 'IFERROR(F4,1)', 'IFNA(F4,1)'
)

Write-Workbook (Join-Path $OutDir 'dc_ibp2/oracle.xlsx') @(
    , @('error_fallback', (New-OracleSheet $fallbackInputs $fallbackSources $fallbackFormulas))
)

# --- dc_l10u/oracle.xlsx ------------------------------------------------------------------------

# dc_l10u/oracle.xlsx is the workbook the Excel capture of bead dc-l10u reads
# (`cargo xtask capture --fixture crates/calc-fml/tests/fixtures/dc_l10u/oracle.xlsx --case
# dc_l10u__reference_results_as_values ...`). It has one sheet, reference_values, whose oracle rows
# pass the reference a function returns (INDEX, XLOOKUP, CHOOSE) to an operator or to a value
# parameter of another function, so the consuming context, not the formula's root, dereferences it.
# Columns A to C hold the constant cells: A1:C3 the numbers 1 to 9; A4 blank, B4 #DIV/0!, C4 -2.5;
# A6:A10 the keys k1 to k5, B6:B10 the numbers 10, 20 and 30, a blank (B9) and #N/A, C6:C10 the
# text a and b, the numeric text 7, TRUE and empty text. Column D holds one formula per oracle row,
# from row 1, in the order the case runs its rows. No formula cell carries a cache.
$referenceInputs = @(
    (New-Number 'A1' '1'), (New-Number 'B1' '2'), (New-Number 'C1' '3'),
    (New-Number 'A2' '4'), (New-Number 'B2' '5'), (New-Number 'C2' '6'),
    (New-Number 'A3' '7'), (New-Number 'B3' '8'), (New-Number 'C3' '9'),
    (New-Error 'B4' '#DIV/0!'), (New-Number 'C4' '-2.5'),
    (New-Text 'A6' 'k1'), (New-Number 'B6' '10'), (New-Text 'C6' 'a'),
    (New-Text 'A7' 'k2'), (New-Number 'B7' '20'), (New-Text 'C7' 'b'),
    (New-Text 'A8' 'k3'), (New-Number 'B8' '30'), (New-Text 'C8' '7'),
    (New-Text 'A9' 'k4'), (New-Logical 'C9' $true),
    (New-Text 'A10' 'k5'), (New-Error 'B10' '#N/A'), (New-Text 'C10' '')
)
$referenceFormulas = @(
    # INDEX's reference result as an operand.
    'INDEX(A1:C3,2,2)+1', '1+INDEX(A1:C3,2,2)', '-INDEX(A1:C3,2,2)',
    'INDEX(A1:C3,2,2)-INDEX(A1:C3,1,1)', 'INDEX(A1:C3,2,2)*INDEX(A1:C3,3,3)',
    'INDEX(A1:C3,2,2)/INDEX(A1:C3,1,2)', 'INDEX(A1:C3,1,2)^2', 'INDEX(A1:C3,2,2)%',
    'INDEX(A1:C3,2,2)&"x"', 'INDEX(A1:C3,2,2)=5', 'INDEX(A1:C3,2,2)>INDEX(A1:C3,3,1)',
    'INDEX(A1:C3,2,2)<>INDEX(A1:C3,1,1)',
    # INDEX(range, MATCH(key, column, 0)) as an operand.
    'INDEX(B:B,MATCH("k3",A:A,0))*2', 'INDEX(B:B,MATCH("k3",A:A,0))+INDEX(B:B,MATCH("k1",A:A,0))',
    'INDEX(C:C,MATCH("k1",A:A,0))&INDEX(C:C,MATCH("k2",A:A,0))',
    # INDEX's reference result inside another function's argument: at that function's value
    # parameter, or at an operator within the argument (/ in ROUND, - in SIGN, > in IF).
    'ABS(INDEX(A1:C4,4,3))', 'ROUND(INDEX(B:B,MATCH("k3",A:A,0))/7,2)', 'INT(INDEX(A1:C4,4,3))',
    'MOD(INDEX(A1:C3,3,3),INDEX(A1:C3,1,2))', 'SIGN(-INDEX(A1:C3,1,1))',
    'IF(INDEX(A1:C3,2,2)>1,"big","small")', 'IF(INDEX(A1:C3,1,1),"t","f")',
    'CHOOSE(INDEX(A1:C3,1,2),"a","b","c")', 'INDEX(A1:C3,INDEX(A1:C3,1,2),1)',
    'MATCH(INDEX(A6:A10,3),A:A,0)', 'ROUND(INDEX(A1:C4,4,3),0)',
    # XLOOKUP's and CHOOSE's reference results as operands and at value parameters.
    'XLOOKUP("k3",A6:A10,B6:B10)+1', '-XLOOKUP("k2",A6:A10,B6:B10)', 'XLOOKUP("k2",A6:A10,C6:C10)&"!"',
    'CHOOSE(2,A1,B1)+1', 'CHOOSE(2,A1,B1)*CHOOSE(1,A2,B2)', 'ABS(CHOOSE(1,C4,C3))',
    # A reference result that points at a blank cell.
    'INDEX(A1:C4,4,1)+1', '-INDEX(A1:C4,4,1)', 'INDEX(A1:C4,4,1)&"x"', 'INDEX(A1:C4,4,1)=0',
    'INDEX(A1:C4,4,1)=""', 'ABS(INDEX(A1:C4,4,1))', 'IF(INDEX(A1:C4,4,1),"t","f")',
    'INDEX(B:B,MATCH("k4",A:A,0))+1', 'XLOOKUP("k4",A6:A10,B6:B10)+1', 'CHOOSE(2,A1,A4)+1',
    'XLOOKUP("k4",A6:A10,B6:B10)&"|"',
    # A reference result that points at an error cell, and a lookup that finds no key.
    'INDEX(A1:C4,4,2)+1', 'INDEX(A1:C4,4,2)&"x"', 'ABS(INDEX(A1:C4,4,2))',
    'IF(INDEX(A1:C4,4,2),"t","f")', 'IFERROR(INDEX(A1:C4,4,2),"caught")',
    'INDEX(B:B,MATCH("k5",A:A,0))*2', 'XLOOKUP("k5",A6:A10,B6:B10)&""', 'CHOOSE(2,A1,B4)*1',
    'INDEX(B:B,MATCH("k9",A:A,0))+1',
    # A reference result that points at text, numeric text, a logical or empty text.
    'INDEX(C6:C10,1)+1', 'INDEX(C6:C10,3)+1', 'INDEX(C6:C10,4)+1', 'INDEX(C6:C10,5)&"|"',
    'INDEX(C6:C10,5)+1'
)

Write-Workbook (Join-Path $OutDir 'dc_l10u/oracle.xlsx') @(
    , @('reference_values', (New-OracleSheet $referenceInputs @() $referenceFormulas))
)
