# The generator of the workbook fixtures under crates/calc-func/tests/fixtures/.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/cfunc_m1_2/oracle.xlsx, <directory>/cfunc_m3_2/oracle.xlsx,
# <directory>/cfunc_m3_4/oracle.xlsx, <directory>/cfunc_m3_5/oracle.xlsx,
# <directory>/dc_q9rb/oracle.xlsx, <directory>/cfunc_m3_6/oracle.xlsx,
# <directory>/dc_e00t/oracle.xlsx, <directory>/dc_fwpz/oracle.xlsx and
# <directory>/cdoc_jpha/oracle.xlsx. Run with this directory as -OutDir it rewrites the committed
# fixtures; the acceptance cases cfunc_m1_2__mixed_operands, cfunc_m3_2__rounding_boundaries,
# cfunc_m3_2__large_integers, cfunc_m3_4__argument_classes, cfunc_m3_5__criteria_semantics,
# dc_q9rb__number_text_for_concatenation, cfunc_m3_6__reference_result,
# cfunc_m3_6__lookup_edges, dc_e00t__digit_labels_as_text, dc_fwpz__logical_functions and
# cdoc_jpha__text_and_number_comparison run it into a fresh directory on Windows and compare its
# output with the committed workbooks byte for byte. cdoc_jpha/oracle.xlsx is written from
# cdoc_jpha/rows.txt beside this script, which the case reads too.
#
# cfunc_m1_2/oracle.xlsx is the workbook the owner's Excel capture of cfunc-m1.2 reads
# (`cargo xtask capture --fixture crates/calc-func/tests/fixtures/cfunc_m1_2/oracle.xlsx ...`),
# cfunc_m3_2/oracle.xlsx the one an Excel capture of cfunc-m3.2's rounding and integer rows reads,
# cfunc_m3_4/oracle.xlsx the one an Excel capture of cfunc-m3.4's aggregate rows reads,
# cfunc_m3_5/oracle.xlsx the one an Excel capture of cfunc-m3.5's conditional rows reads,
# dc_q9rb/oracle.xlsx the one an Excel capture of dc-q9rb's number-text rows reads,
# cfunc_m3_6/oracle.xlsx the one an Excel capture of cfunc-m3.6's lookup rows reads,
# dc_e00t/oracle.xlsx the one an Excel capture of dc-e00t's digit-label rows reads,
# dc_fwpz/oracle.xlsx the one an Excel capture of dc-fwpz's logical rows reads, and
# cdoc_jpha/oracle.xlsx the one an Excel capture of cdoc-jpha's comparison rows reads. Each has one
# sheet per case, named after the case: the cells the case's rows read, as the case's test sheet
# holds them, and in column D one formula per oracle row, from row 1, in the order the case runs
# its rows. Each case checks that its sheet holds exactly those formulas and cells; the sheet
# lookup_edges of cfunc_m3_6/oracle.xlsx also holds source formulas, each a dynamic-array formula
# over its one cell, and dc_e00t's workbook also has a sheet value_readings of Excel's VALUE over
# its generated texts, which no row computes (see its section). No formula cell carries a cache:
# Excel calculates them.
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

# e with acute, small and capital. (PowerShell variable names ignore case, so the names differ.)
$eSmall = [string][char]0xE9
$eCapital = [string][char]0xC9

# Sheet mixed_operands: the cells of cfunc_m1_2__mixed_operands' test sheet (A5 and A6 blank; C2
# holds a text of 32767 characters, the longest a cell holds, and C1 one character less; C3 and C4
# hold two numbers of 14 significant digits that differ in the last; C5 holds 0, C6 the text
# 1e-400 and C7 the text 1e-309), and its oracle rows' formulas in its order, the rows whose
# behaviour another bead owns included.
$mixedInputs = @(
    (New-Text 'A1' '2'),
    (New-Logical 'A2' $true),
    (New-Number 'A3' '5'),
    (New-Text 'A4' 'a'),
    (New-Number 'A7' '10'),
    (New-Number 'B1' '0.5'),
    (New-Number 'B2' '-2'),
    (New-Number 'B3' '10'),
    (New-Text 'C1' ('a' * 32766)),
    (New-Text 'C2' ('a' * 32767)),
    (New-Number 'C3' '123456789012.34'),
    (New-Number 'C4' '123456789012.33'),
    (New-Number 'C5' '0'),
    (New-Text 'C6' '1e-400'),
    (New-Text 'C7' '1e-309')
)
# The literals of both sheets are ones Excel loads from a file unchanged (Excel 16.0.20430, opening
# the workbook with CorruptLoad 0): a number above 9.99999999999999E+307, the largest a cell can
# be given, is not loaded, written 1E308 or 1E+308 alike (Excel asks to repair the workbook, and
# the repair removes the formula); the literal 2.2250738585072014E-308 loads as 0; and the formula
# -0 loads as the literal 0, so the negation of 0 is written -(0).
$mixedFormulas = @(
    # Arithmetic operands through worksheet number coercion, decimal text included; then text
    # that Excel's number-text rules may read (cfml-m2.6).
    '"1"+1', 'A1+1', 'TRUE+1', 'A2*5', '"a"+1', '""+1', 'A5+1', '"TRUE"+1', '" 1.5 "*2',
    '"1e3"/1', '"-0"+0', '"123456789012345"+0', '5-"3"', '"+5"+0', '".5"+0', '"5."+0',
    '"1E+3"+0', '"0.1"+0', '"1e-400"+0',
    '"1/2"+0', '"$5"+0', '"1,000"+0', '"50%"+0', '"1234567890123456"+0',
    '"1.2345678901234567"+0', '0+"1e400"',
    # Results (a subnormal one: cfunc-m3.23), ^ (2^3, 10^-2 and 2^0.5: cfunc-m3.3 and
    # cfunc-m3.23), the unary operators, percent (x/100 against x*0.01 at 35% and 57%), and a +
    # or - near zero (dc-r4ci: the formula's final operation), with boundary rows for its
    # measurement.
    '9.99999999999999E+307*10', '0*-1', '1/0', 'A5/A6', '2.2250738585073E-308/2', '"2"^2',
    'TRUE^2', '2^3', '10^-2', '2^0.5', '3^1', '-(0)', '-"2"', '-TRUE', '-A5', '-""', '-2', '+"a"', '+TRUE', '+A5',
    '50%', '"50"%', 'TRUE%', '7%', '35%', '57%',
    '0.5-0.4-0.1', '1*(0.5-0.4-0.1)', '1.333+1.225-1.333-1.225', '1E15+1-1E15', '0.1+0.2-0.3',
    '1+1E-15-1', 'C3-C4',
    # Comparison; text collation and nearly equal numbers are cdoc-jpha's.
    '"1"=1', 'A1<>2', '1<"a"', '"a"<TRUE', 'FALSE>9.99999999999999E+307', '"a"="A"', 'A5=0',
    'A5=""', 'A5=FALSE', 'A5=A6', 'A5<1', '"abc"<"abd"', '"B">"a"', '"a1"<"aa"', '"10"<"9"', '""<"a"',
    '"a"<="A"', '1>=1', '1<=0', 'TRUE>FALSE', ('"' + $eSmall + '"="' + $eSmall + '"'), '1=2',
    '0.1+0.2=0.3', ('"' + $eSmall + '"="' + $eCapital + '"'), '"a-b"<"ab"',
    '"apple pie"<"banana"', '"a"<"B"', '"A"="a"', '1+1E-15=1',
    # Concatenation, a number's text included (dc-q9rb measures it).
    '"a"&"b"', '"a"&TRUE', '"a"&FALSE', 'A5&"x"', 'A2&A4', '"a"&1', '1&"a"', 'C1&"b"',
    'C2&"b"',
    # SUM.
    'SUM("2",TRUE)', 'SUM(A1:A3)', 'SUM(A1:A3,"2",TRUE)', 'SUM({1,"2",TRUE})', 'SUM("a")',
    'SUM(A4)', 'SUM(A5)', 'SUM("")', 'SUM(1,)',
    'SUM(9.99999999999999E+307,9.99999999999999E+307)',
    'SUM(9.99999999999999E+307,9.99999999999999E+307,-9.99999999999999E+307)', 'SUM(B1:B3)', 'SUM((A3,A7))', 'SUM(A:A)',
    # IF; the last row is a legacy formula, which Excel loads as =@IF({TRUE,FALSE},1,2) (cfml-m3.4).
    'IF("TRUE",1,2)', 'IF("false",1,2)', 'IF("x",1,2)', 'IF("",1,2)', 'IF("1",1,2)',
    'IF(A5,1,2)', 'IF(0,1)', 'IF(2,"a",1/0)', 'IF(TRUE,,2)', 'IF(FALSE,1,)', 'IF(,1,2)',
    'IF({TRUE,FALSE},1,2)',
    # Number text at the edges of the range Excel reads, a zero mantissa, a mantissa with leading
    # or trailing zeros and a negative one among them; three products show whether text in
    # binary64's subnormal range reads as 0.
    '"1e308"+0', '"9.99999999999999e307"+0', '"0e308"+0', '"0e309"+0', '"0e-308"+0',
    '"0e-309"+0', '"0.0e309"+0', '"0.0e-308"+0', '"1e-309"+0', '"9e-310"+0', '"0.01e309"+0',
    '"1000e-310"+0', '"0.01e-308"+0', '"1e-309"*1E+300', '"2.2250738585072E-308"*1E+300',
    '"-1e308"+0', '"-1e-309"*1E+300',
    # & cut where the cut would part a surrogate pair (U+1F600 after C1's 32766 characters).
    ('C1&"' + [char]::ConvertFromUtf32(0x1F600) + '"'),
    # Number text beyond the range Excel reads and in binary64's subnormal range, read by COUNT
    # (cfunc-m3.4's function) and by COUNTIF (cfunc-m3.5's function), in a criterion and in a cell
    # of the range; their own workbooks hold no such row.
    'COUNT("1e-400")', 'COUNTIF(C5,"1e-400")', 'COUNTIF(C5,"<1e308")', 'COUNTIF(C6,0)',
    'COUNT("1e-309")', 'COUNTIF(C7,"1e-309")'
)

# Sheet error_order: the cells of cfunc_m1_2__error_order's test sheet, and its oracle rows'
# formulas in its order.
$orderInputs = @(
    (New-Number 'A1' '1'),
    (New-Error 'B1' '#N/A'),
    (New-Error 'A2' '#DIV/0!'),
    (New-Number 'B2' '2')
)
$orderFormulas = New-Object System.Collections.ArrayList
foreach ($op in @('+', '-', '*', '/', '^', '=', '<>', '<', '<=', '>', '>=', '&')) {
    foreach ($pair in @(@('#N/A', '#DIV/0!'), @('#DIV/0!', '#N/A'), @('1', '#REF!'), @('#N/A', '"a"'), @('"a"', '#N/A'))) {
        [void]$orderFormulas.Add($pair[0] + $op + $pair[1])
    }
}
foreach ($formula in @(
    '#N/A/0', '"a"/0', '0/"a"', '0^#N/A', '1&#N/A', '-#N/A', '#DIV/0!%', '+#REF!',
    'SUM(#N/A,#DIV/0!)', 'SUM(1,"a",#N/A)', 'SUM(A1:B2)', 'SUM((A2:B2,A1:B1))',
    'SUM(A1:B2,#NULL!)', 'SUM(#REF!,A1:B2)',
    'SUM(9.99999999999999E+307,9.99999999999999E+307,#N/A)',
    'IF(#N/A,#DIV/0!,#REF!)', 'IF(TRUE,#DIV/0!,#N/A)', 'IF(FALSE,#DIV/0!,#N/A)',
    'IF("x",#DIV/0!,#N/A)',
    # SUM: an array's error against a direct argument's error or coercion error and against a
    # reference's error, each way round; a direct coercion error against a reference's error; a
    # reference's error after an overflow of the running total.
    'SUM({1,#REF!},#N/A)', 'SUM(#N/A,{1,#REF!})', 'SUM({1,#REF!},"a")', 'SUM({1,#REF!},A1:B2)',
    'SUM(A1:B2,{1,#REF!})', 'SUM(B1,"a")', 'SUM(A1:B2,"a")',
    'SUM(9.99999999999999E+307,9.99999999999999E+307,B1)'
)) {
    [void]$orderFormulas.Add($formula)
}

Write-Workbook (Join-Path $OutDir 'cfunc_m1_2/oracle.xlsx') @(
    @('mixed_operands', (New-OracleSheet $mixedInputs $mixedFormulas)),
    @('error_order', (New-OracleSheet $orderInputs $orderFormulas))
)

# --- cfunc_m3_4/oracle.xlsx ---------------------------------------------------------------------

# Sheet argument_classes: the cells of cfunc_m3_4__argument_classes' test sheet (A1:A8 one of each
# class, A5 blank and A6 empty text held as a constant; B1:B3 an error between two numbers; C1:C8
# numbers), and its oracle rows' formulas in its order.
$classInputs = @(
    (New-Number 'A1' '1'),
    (New-Text 'A2' '2'),
    (New-Logical 'A3' $true),
    (New-Text 'A4' 'a'),
    (New-Text 'A6' ''),
    (New-Number 'A7' '4'),
    (New-Logical 'A8' $false),
    (New-Number 'B1' '5'),
    (New-Error 'B2' '#DIV/0!'),
    (New-Number 'B3' '-3'),
    (New-Number 'C1' '10'),
    (New-Number 'C2' '10'),
    (New-Number 'C3' '10'),
    (New-Number 'C4' '10'),
    (New-Number 'C5' '10'),
    (New-Number 'C6' '10'),
    (New-Number 'C7' '0.5'),
    (New-Number 'C8' '10')
)
$classFormulas = @(
    'SUM(A1:A8)', 'SUM(1E16,1,-1E16)', 'SUM(B1:B3)', 'SUM(2,TRUE)', 'SUM(2,"3")',
    'PRODUCT(2,3,4)', 'PRODUCT(A1:A8)', 'PRODUCT({2,"x",TRUE})', 'PRODUCT("2",TRUE)', 'PRODUCT("a")',
    'PRODUCT(A3:A6)', 'PRODUCT(B1:B3)', 'PRODUCT(2,)', 'PRODUCT(1E-200,1E-200)',
    'SUMSQ(2,3,4)', 'SUMSQ("2",TRUE)', 'SUMSQ("a")', 'SUMSQ(A1:A8)', 'SUMSQ({3,TRUE})', 'SUMSQ(B1:B3)',
    'SUMSQ(1E-200)',
    'SUMPRODUCT({1,2},{3,4})', 'SUMPRODUCT({1,4,9})', 'SUMPRODUCT({1,TRUE},{3,4})',
    'SUMPRODUCT(A1:A8,C1:C8)', 'SUMPRODUCT(A7:A8,C6:C7)', 'SUMPRODUCT({1,2},3)',
    'SUMPRODUCT(B1:B3,C1:C3)', 'SUMPRODUCT({1,2},{1,2,3},{1,#N/A})', 'SUMPRODUCT(2,3)', 'SUMPRODUCT("3")',
    'SUMPRODUCT(A1:A2,)', 'SUMPRODUCT((A1,A7))',
    'SUMPRODUCT(A1:A3,C1:C2)', 'SUMPRODUCT(A1:A2,{1,2})', 'SUMPRODUCT(C1:C2,{1;2})', 'SUMPRODUCT(1E-200,1E-200)',
    'COUNT(A1:A8)', 'COUNT(B1:B3)', 'COUNT((A1:A2,B2:B3))', 'COUNT(1,"2",TRUE)', 'COUNT("a",#N/A,"")',
    'COUNT({1,"2",TRUE})', 'COUNT(1,)', 'COUNT("1/2")',
    'COUNTA(A1:A8)', 'COUNTA(B1:B3)', 'COUNTA(A5)', 'COUNTA("a",#N/A,FALSE,"")', 'COUNTA({1,"2",TRUE})',
    'COUNTA(1,)',
    'COUNTBLANK(A5)', 'COUNTBLANK(B1:B3)', 'COUNTBLANK(A1:A8)', 'COUNTBLANK(A1:C8)', 'COUNTBLANK((A5,A6))',
    'COUNTBLANK(#REF!)',
    'AVERAGE(1,2,5)', 'AVERAGE(A1:A8)', 'AVERAGE((A7,B3))', 'AVERAGE(B1:B3)', 'AVERAGE("a")',
    'AVERAGE("2",TRUE)', 'AVERAGE(A3:A6)', 'AVERAGE({1,"2",TRUE})', 'AVERAGE(1,)', 'AVERAGE(1E308,1E308)',
    'AVERAGEA("2",TRUE)', 'AVERAGEA((A3,A8))', 'AVERAGEA(A1:A8)', 'AVERAGEA(A5)', 'AVERAGEA("a")',
    'AVERAGEA(B1:B3)', 'AVERAGEA({1,"2",TRUE})',
    'MIN(A1:A8)', 'MIN("2",TRUE)', 'MIN(A3:A6)', 'MIN(B1:B3)', 'MIN(5,"a")', 'MIN({TRUE,"1"})', 'MIN(1,)',
    'MAX(A1:A8)', 'MAX("2",TRUE)', 'MAX((A4,B3))', 'MAX(B1:B3)', 'MAX(A3:A6)', 'MAX(-1,)',
    'MINA((A3,A7))', 'MINA("2",TRUE)', 'MINA(A1:A4)', 'MINA(A5)', 'MINA(B1:B3)',
    'MAXA((A3,B3))', 'MAXA("2",TRUE)', 'MAXA((A4,B3))', 'MAXA("a")', 'MAXA({-1,"a"})'
)

# Sheet sparse_blanks: the cells of cfunc_m3_4__sparse_blanks' test sheet (column A down to its
# last row, A2 empty text held as a constant; two cells of column B; one of column C), and its
# oracle rows' formulas, which read whole columns, in its order.
$sparseInputs = @(
    (New-Number 'A1' '3'),
    (New-Text 'A2' ''),
    (New-Text 'A3' 'x'),
    (New-Number 'A1000' '5'),
    (New-Number 'A1048576' '7'),
    (New-Number 'B1' '2'),
    (New-Number 'B1048576' '10'),
    (New-Logical 'C500' $true)
)
$sparseFormulas = @(
    'COUNTBLANK(B:B)', 'COUNTBLANK(A:A)', 'COUNTBLANK(A:C)', 'COUNT(A:A)', 'COUNTA(A:A)', 'SUM(A:A)',
    'PRODUCT(A:A)', 'SUMSQ(A:A)', 'AVERAGE(A:A)', 'AVERAGEA(A:A)', 'MIN(A:A)', 'MAX(A:A)', 'MINA(A:A)',
    'MAXA(B:B)', 'SUMPRODUCT(A:A,B:B)'
)

Write-Workbook (Join-Path $OutDir 'cfunc_m3_4/oracle.xlsx') @(
    @('argument_classes', (New-OracleSheet $classInputs $classFormulas)),
    @('sparse_blanks', (New-OracleSheet $sparseInputs $sparseFormulas))
)

# --- cfunc_m3_2/oracle.xlsx ---------------------------------------------------------------------

# Sheet rounding_boundaries: the cells of cfunc_m3_2__rounding_boundaries' test sheet (A1, A2, A4,
# A5 and A6 hold numbers whose shortest decimal needs more than 15 significant digits, so they
# cannot be typed; A3 holds the INT page's example cell; A6 is a normal number whose remainder by
# 5.70820472448858E-308 is too small for binary64), and its oracle rows' formulas in its order.
$boundaryInputs = @(
    (New-Number 'A1' '2.6749999999999994'),
    (New-Number 'A2' '0.30000000000000004'),
    (New-Number 'A3' '19.5'),
    (New-Number 'A4' '1.0049999999999997'),
    (New-Number 'A5' '2.9999999999999996'),
    (New-Number 'A6' '1.141640944897716E-307')
)
$boundaryFormulas = @(
    'ROUND(2.15,1)', 'ROUND(2.149,1)', 'ROUND(-1.475,2)', 'ROUND(21.5,-1)', 'ROUND(626.3,-3)',
    'ROUND(1.98,-1)', 'ROUND(-50.55,-2)', 'ROUND(2.5,0)', 'ROUND(-2.5,0)', 'ROUND(0.5,0)',
    'ROUND(-0.5,0)', 'ROUND(-0.4,0)', 'ROUND(-0.0001,2)', 'ROUND(25,-1)', 'ROUND(-15,-1)',
    'ROUND(1250,-2)', 'ROUND(1234,-2)', 'ROUND(0.5,-1)', 'ROUND(2.675,2)', 'ROUND(-2.675,2)',
    'ROUND(1.005,2)', 'ROUND(A1,2)', 'ROUND(A4,2)', 'ROUNDUP(A2,1)', 'ROUNDDOWN(0.57,2)',
    'TRUNC(0.57,2)', 'TRUNC(A5)', 'INT(A5)', 'ROUNDUP(3.2,0)', 'ROUNDUP(76.9,0)',
    'ROUNDUP(3.14159,3)', 'ROUNDUP(-3.14159,1)', 'ROUNDUP(31415.92654,-2)', 'ROUNDUP(-0.1,0)',
    'ROUNDDOWN(3.2,0)', 'ROUNDDOWN(76.9,0)', 'ROUNDDOWN(3.14159,3)', 'ROUNDDOWN(-3.14159,1)',
    'ROUNDDOWN(31415.92654,-2)', 'ROUNDDOWN(-0.5,0)', 'ROUND(1.2345,2.9)', 'ROUND(1234.5,-1.9)',
    'ROUND(1.5,400)', 'ROUND(1.5,-400)', 'ROUNDUP(1.5,-400)', 'TRUNC(8.9)', 'TRUNC(-8.9)',
    'TRUNC(0.45)', 'TRUNC(-4.3)', 'INT(-4.3)', 'INT(8.9)', 'INT(-8.9)', 'INT(A3)', 'TRUNC(-0.5)',
    'TRUNC(8.9,)', 'MROUND(10,3)', 'MROUND(-10,-3)', 'MROUND(1.3,0.2)', 'MROUND(5,-2)',
    'MROUND(-5,2)', 'MROUND(7.5,5)', 'MROUND(6.05,0.1)', 'MROUND(7.05,0.1)',
    'MROUND(-6.05,-0.1)', 'MROUND(5,0)',
    'CEILING(2.5,1)', 'CEILING(-2.5,-2)', 'CEILING(-2.5,2)', 'CEILING(1.5,0.1)',
    'CEILING(0.234,0.01)', 'CEILING(4.3,2)', 'CEILING(-4.3,2)', 'CEILING(-4.3,-2)',
    'CEILING(4.3,-2)', 'CEILING(5,0)', 'CEILING(0.3,0.1)', 'CEILING(-0.5,1)', 'FLOOR(3.7,2)',
    'FLOOR(-2.5,-2)', 'FLOOR(2.5,-2)', 'FLOOR(1.58,0.1)', 'FLOOR(0.234,0.01)', 'FLOOR(4.3,2)',
    'FLOOR(-4.3,2)', 'FLOOR(-4.3,-2)', 'FLOOR(4.3,-2)', 'FLOOR(-2.5,2)', 'FLOOR(5,0)',
    'FLOOR(0.15,0.05)', 'FLOOR(0.3,0.1)', 'FLOOR(A2,0.1)', 'MOD(3,2)', 'MOD(-3,2)', 'MOD(3,-2)',
    'MOD(-3,-2)', 'MOD(3,0)', 'MOD(-7,2.5)', 'MOD(0.3,0.1)', 'MOD(1,0.1)', 'QUOTIENT(5,2)',
    'QUOTIENT(4.5,3.1)', 'QUOTIENT(-10,3)', 'QUOTIENT("a",2)', 'QUOTIENT(1,0)',
    'QUOTIENT(0.3,0.1)', 'QUOTIENT(-1,3)', 'EVEN(1.5)', 'EVEN(3)', 'EVEN(2)', 'EVEN(-1)',
    'EVEN(-0.5)', 'EVEN(-1.5)', 'EVEN(0)', 'EVEN("a")', 'ODD(1.5)', 'ODD(3)', 'ODD(2)', 'ODD(-1)',
    'ODD(-2)', 'ODD(-1.5)', 'ODD(0)', 'ODD("a")', 'ABS(2)', 'ABS(-2)', 'ABS(-2.5)', 'ABS(" -2 ")',
    'ABS("asd")', 'ABS(TRUE)', 'ABS(#DIV/0!)', 'SIGN(10)', 'SIGN(-0.00001)', 'SIGN(0)',
    'ROUND(#N/A,#DIV/0!)', 'MOD(#N/A,0)', 'FLOOR(1,#REF!)', 'CEILING("2.5",TRUE)',
    'CEILING("a",1)', 'ABS({-1,2})', 'ROUND(1.5,{0,1})', 'ROUNDDOWN(2.5E-308,308)',
    'MOD(A6,5.70820472448858E-308)'
)

# Sheet large_integers: the cells of cfunc_m3_2__large_integers' test sheet (2^53 and its
# neighbours, 2^52-0.5 of both signs, 1E15+0.5, the largest binary64 of both signs, 2^63, 2^60,
# 2^64 and 2^53-2, each written as the shortest decimal that reads back as it, of 15 to 17
# significant digits), and its oracle rows' formulas in its order.
$largeInputs = @(
    (New-Number 'A1' '9007199254740992'),
    (New-Number 'A2' '9007199254740991'),
    (New-Number 'A3' '9007199254740994'),
    (New-Number 'A4' '4503599627370495.5'),
    (New-Number 'A5' '-4503599627370495.5'),
    (New-Number 'A6' '1000000000000000.5'),
    (New-Number 'A7' '1.7976931348623157E+308'),
    (New-Number 'A8' '-1.7976931348623157E+308'),
    (New-Number 'A9' '9.223372036854776E+18'),
    (New-Number 'A10' '1.152921504606847E+18'),
    (New-Number 'A11' '1.8446744073709552E+19'),
    (New-Number 'A12' '9007199254740990')
)
$largeFormulas = @(
    'INT(A4)', 'INT(A5)', 'INT(A1)', 'INT(A6)', 'INT(A9)', 'INT(A11)', 'INT(1E20)', 'INT(A7)',
    'INT(A8)', 'TRUNC(A5)', 'TRUNC(A6)', 'TRUNC(A9)', 'TRUNC(A8)',
    'TRUNC(9.99999999999999E307,-307)', 'TRUNC(A7,-308)', 'ROUND(A4,0)', 'ROUND(A5,0)',
    'ROUND(A6,0)', 'ROUND(A1,0)', 'ROUND(A3,-1)', 'ROUND(999999999999999,-1)', 'ROUND(A9,-3)',
    'ROUND(A7,0)', 'ROUND(9.99999999999999E307,-307)', 'ROUND(A7,-308)', 'ROUNDDOWN(A7,-308)',
    'ROUND(1.5,1E300)', 'ROUND(1.5,-1E300)', 'ROUNDUP(1.5,-1E300)', 'MOD(A1,3)', 'MOD(A3,3)',
    'MOD(999999999999999,2)', 'MOD(1E15,7)', 'MOD(-1E15,7)', 'MOD(A10,1024)', 'MOD(A10,3)',
    'MOD(1E300,7)', 'MOD(1E23,1E8)', 'MOD(A7,3)', 'MOD(A7,A8)', 'MOD(-5,1E308)', 'QUOTIENT(A1,1)',
    'QUOTIENT(A3,2)', 'QUOTIENT(1E16,3)', 'QUOTIENT(-1E16,3)', 'QUOTIENT(1E20,3)',
    'QUOTIENT(A7,0.5)', 'QUOTIENT(1E300,1E-10)', 'EVEN(A2)', 'EVEN(A5)', 'EVEN(A1)',
    'EVEN(999999999999999)',
    'EVEN(A9)', 'EVEN(A7)', 'EVEN(A8)', 'ODD(A2)', 'ODD(A12)', 'ODD(A4)', 'ODD(1E15)', 'ODD(A1)',
    'ODD(A3)', 'ODD(A9)', 'ODD(A7)', 'CEILING(A4,1)', 'FLOOR(A5,1)', 'FLOOR(A7,1)',
    'FLOOR(A10,1024)', 'CEILING(A7,1E308)', 'FLOOR(A8,1E308)', 'CEILING(1E-300,1E300)',
    'FLOOR(-1E-300,1E300)', 'MROUND(A6,1)', 'MROUND(A1,3)', 'MROUND(A7,0.5)',
    'ABS(A8)', 'ABS(A5)',
    'SIGN(A8)'
)

Write-Workbook (Join-Path $OutDir 'cfunc_m3_2/oracle.xlsx') @(
    @('rounding_boundaries', (New-OracleSheet $boundaryInputs $boundaryFormulas)),
    @('large_integers', (New-OracleSheet $largeInputs $largeFormulas))
)

# --- cfunc_m3_5/oracle.xlsx ---------------------------------------------------------------------

# MAXIFS and MINIFS came after the file format's first version, so a file writes them with the
# _xlfn. prefix, without which Excel does not recognize them.
function Get-Stored([string]$Formula) {
    return $Formula.Replace('MAXIFS(', '_xlfn.MAXIFS(').Replace('MINIFS(', '_xlfn.MINIFS(')
}

# Sheet criteria_semantics: the cells of cfunc_m3_5__criteria_semantics' test sheet (A1:A16 one
# value of each kind the criteria language tells apart, A8 blank and A9 empty text held as a
# constant; B1:B16 the powers of two 1 to 32768; C1:C16 the row numbers; A18:A22 and B18:B22 a
# target with number text, a logical and an error; A23 text beyond ASCII; A24 a number within a
# relative 1e-13 of 0.3; A25 a label holding digits; A26 text holding a ~, A27 FALSE and A28
# #VALUE!), and its oracle rows' formulas in its order.
$criteriaInputs = New-Object System.Collections.ArrayList
foreach ($cell in @(
    (New-Text 'A1' 'apple'),
    (New-Text 'A2' 'Apple Pie'),
    (New-Text 'A3' 'banana'),
    (New-Number 'A4' '5'),
    (New-Text 'A5' '5'),
    (New-Number 'A6' '10'),
    (New-Logical 'A7' $true),
    (New-Text 'A9' ''),
    (New-Text 'A10' 'a*b'),
    (New-Text 'A11' 'axb'),
    (New-Error 'A12' '#N/A'),
    (New-Text 'A13' 'abc'),
    (New-Text 'A14' 'a?c'),
    (New-Number 'A15' '0'),
    (New-Text 'A16' 'APPLE')
)) { [void]$criteriaInputs.Add($cell) }
for ($row = 1; $row -le 16; $row++) {
    [void]$criteriaInputs.Add((New-Number ('B' + $row) ([string][math]::Pow(2, $row - 1))))
    [void]$criteriaInputs.Add((New-Number ('C' + $row) ([string]$row)))
}
foreach ($cell in @(
    (New-Text 'A18' 'k'),
    (New-Text 'A19' 'k'),
    (New-Text 'A20' 'k'),
    (New-Text 'A21' 'k'),
    (New-Text 'A22' 'j'),
    (New-Text 'A23' ('caf' + $eSmall)),
    (New-Number 'A24' '0.30000000000000004'),
    (New-Text 'A25' '2016-17 Winter'),
    (New-Text 'A26' 'a~b'),
    (New-Logical 'A27' $false),
    (New-Error 'A28' '#VALUE!'),
    (New-Number 'B18' '3'),
    (New-Text 'B19' '4'),
    (New-Logical 'B20' $true),
    (New-Error 'B21' '#DIV/0!'),
    (New-Number 'B22' '100')
)) { [void]$criteriaInputs.Add($cell) }
$criteriaFormulas = @(
    'COUNTIF(A1:A16,"apple")', 'COUNTIF(A1:A16,"=apple")', 'COUNTIF(A1:A16,"<>apple")',
    'COUNTIF(A1:A16,"app*")', 'COUNTIF(A1:A16,"*")', 'COUNTIF(A1:A16,"?????")',
    'COUNTIF(A1:A16,"a*b")', 'COUNTIF(A1:A16,"a~*b")', 'COUNTIF(A1:A16,"a?c")',
    'COUNTIF(A1:A16,"a~?c")', 'COUNTIF(A1:A16,5)', 'COUNTIF(A1:A16,"5")', 'COUNTIF(A1:A16,">=5")',
    'COUNTIF(A1:A16,"<5")', 'COUNTIF(A1:A16,"<>5")', 'COUNTIF(A1:A16,"="&A16)', 'COUNTIF(A1:A16,"")', 'COUNTIF(A1:A16,"=")',
    'COUNTIF(A1:A16,"<>")', 'COUNTIF(A1:A16,TRUE)', 'COUNTIF(A1:A16,"TRUE")',
    'COUNTIF(A1:A16,"#N/A")', 'COUNTIF(A1:A16,"<>#N/A")', 'COUNTIF(A1:A16,A8)',
    'COUNTIF(A1:A16,A1)', 'COUNTIF(A3:A6,"<b")', 'COUNTIF(A1:A16,"*5*")', 'COUNTIF(A1:A16,"<>*")',
    'COUNTIF(A1:A16,"<b")', 'COUNTIF(A1:A16,"Q1")', 'COUNTIF(A1:A16,"1/2")',
    'COUNTIF(A1:A16,"#VALUE!")', 'COUNTIF(A1:A16,"a~b")', 'COUNTIF(A1:A16,#N/A)', 'COUNTIF(A1:A16,)',
    'COUNTIF(A1:A16,"<TRUE")', ('COUNTIF(A1:A16,"' + ('a' * 256) + '")'),
    ('COUNTIF(A23,"caf' + $eSmall + '")'), ('COUNTIF(A23,"CAF' + $eCapital + '")'),
    'COUNTIF(A22:A23,"*")', 'COUNTIF(A22:A23,"caf*")', 'COUNTIF(A24,0.3)', 'COUNTIF(A18:A22,"K")',
    'COUNTIF(B18:B22,">0")',
    'SUMIF(A1:A16,"apple",B1:B16)', 'SUMIF(A1:A16,"a*",B1:B16)', 'SUMIF(A1:A16,"",B1:B16)',
    'SUMIF(A1:A16,"<>",B1:B16)', 'SUMIF(A1:A16,5,B1:B16)', 'SUMIF(A1:A16,"<>5",B1:B16)', 'SUMIF(A1:A16,">=5")',
    'SUMIF(A1:A16,"<>#N/A",B1:B16)', 'SUMIF(A1:A16,TRUE,B1:B16)', 'SUMIF(A1:A16,A8,B1:B16)',
    'SUMIF(C1:C16,">8")', 'SUMIF(C1:C16,">8",B1)', 'SUMIF(C1:C16,"<3",B5:B6)',
    'SUMIF(A18:A20,"k",B18:B20)', 'SUMIF(A18:A22,"k",B18:B22)', 'SUMIF(A18:A22,"j",B18:B22)',
    'SUMIFS(B1:B16,A1:A16,"a*",C1:C16,">10")', 'SUMIFS(B1:B16,A1:A16,"<>apple",C1:C16,"<=4")',
    'SUMIFS(B1:B16,A1:A16,"a?c")', 'SUMIFS(B18:B20,A18:A20,"k")',
    'COUNTIFS(A1:A16,"a*",C1:C16,">10")', 'COUNTIFS(A1:A16,"<>apple",C1:C16,"<=4")',
    'COUNTIFS(C1:C16,">3",C1:C16,"<6")', 'COUNTIFS(A1:A16,"=",C1:C16,">0")',
    'AVERAGEIF(C1:C16,">12")', 'AVERAGEIF(A1:A16,"a*",C1:C16)', 'AVERAGEIF(A1:A16,"zzz",B1:B16)',
    'AVERAGEIF(A1:A16,">=5")', 'AVERAGEIF(A18:A20,"k",B18:B20)',
    'AVERAGEIFS(C1:C16,A1:A16,"a*",C1:C16,">10")', 'AVERAGEIFS(B1:B16,A1:A16,"zzz")',
    'AVERAGEIFS(B18:B20,A18:A20,"k")', 'AVERAGEIFS(C1:C16,A1:A16,"<1")',
    'MAXIFS(B1:B16,A1:A16,"a*")', 'MINIFS(B1:B16,A1:A16,"a*")', 'MAXIFS(B1:B16,A1:A16,"zzz")',
    'MINIFS(C1:C16,A1:A16,">=5")', 'MAXIFS(C1:C16,A1:A16,A8)', 'MAXIFS(B18:B22,A18:A22,"k")',
    'MINIFS(B18:B22,A18:A22,"j")', 'AVERAGEIF(A1:A16,TRUE,B1:B16)', 'AVERAGEIF(A1:A16,"<>apple",B1:B16)',
    'SUMIFS(B1:B16,A1:A16,"="&A25)', 'COUNTIF(A23:A25,">0")', 'COUNTIF(A25,"<>2016")', 'COUNTIF(A25,2016)',
    'COUNTIF(C1:C16,"="&C4)', 'SUMIF(A1:A16,,B1:B16)', 'COUNTIF(A26,"a~b")', 'COUNTIF(A27,"<TRUE")',
    'COUNTIF(A1:A16,">=")', 'COUNTIF(B18:B22,"<#N/A")', 'COUNTIF(A28,"#VALUE!")'
)

# Sheet mismatched_shapes: the cells of cfunc_m3_5__mismatched_shapes' test sheet (A2:A5 and
# B3:B6 the MAXIFS page's example of ranges of one size at different rows, C2:C5 a second
# criterion aligned with A2:A5, and a cell of each column in the last row), and its oracle rows'
# formulas, small ranges first, then whole columns, then two shaped third arguments inside the
# reference given, in its order.
$shapeInputs = @(
    (New-Number 'A2' '10'),
    (New-Number 'A3' '1'),
    (New-Number 'A4' '100'),
    (New-Number 'A5' '1'),
    (New-Number 'A1048576' '7'),
    (New-Text 'B3' 'a'),
    (New-Text 'B4' 'a'),
    (New-Text 'B5' 'b'),
    (New-Text 'B6' 'a'),
    (New-Text 'B1048576' 'a'),
    (New-Text 'C2' 'x'),
    (New-Text 'C3' 'y'),
    (New-Text 'C4' 'x'),
    (New-Text 'C5' 'x'),
    (New-Text 'C1048576' 'x')
)
$shapeFormulas = @(
    'MAXIFS(A2:A5,B3:B6,"a")', 'MINIFS(A2:A5,B3:B6,"a")', 'MAXIFS(A2:A5,B2:C6,"a")',
    'MINIFS(A2:A5,B3:C4,"a")', 'SUMIFS(A2:A5,B3:B6,"a")', 'SUMIFS(A2:A5,B3:B5,"a")',
    'SUMIFS(A2:A5,B3:B6,"a",C2:C5,"x")', 'SUMIFS(A2:A5,B3:B6,"a",C2:C4,"x")',
    'SUMIFS(A2:A5,B3:B5,"Q1")', 'COUNTIFS(B3:B6,"a",C2:C5,"x")', 'COUNTIFS(B3:B6,"a",C2:C6,"x")',
    'AVERAGEIFS(A2:A5,B3:B6,"a")', 'AVERAGEIFS(A2:A5,B3:B6,"a",C2:C3,"x")',
    'SUMIF(B3:B6,"a",A2)', 'SUMIF(B3:B6,"a",A2:A3)', 'SUMIF(B3:B6,"a",A2:C9)',
    'AVERAGEIF(B3:B6,"a",A2)', 'AVERAGEIF(B3:B6,"b",A2:B2)', 'SUMIF(B3:C4,"a",A2)',
    'COUNTIFS(B3:C4,"a",A2:B3,">5")', 'SUMIF(B3:B6,"a",A1048575)', 'SUMIF((B3:B4,B6),"a")',
    'COUNTIF(A:A,">5")', 'COUNTIF(B:B,"a")', 'COUNTIF(B:B,"<>a")', 'COUNTIF(A:A,"")', 'COUNTIF(A:A,"<>1")',
    'SUMIF(B:B,"a",A:A)', 'SUMIF(B:B,"a",A1)', 'SUMIFS(A:A,B:B,"a",C:C,"x")',
    'COUNTIFS(A:A,">0",C:C,"<>y")', 'COUNTIFS(B:B,"<>a",C:C,"<>y")', 'AVERAGEIFS(A:A,B:B,"a")',
    'MAXIFS(A:A,B:B,"a")', 'MINIFS(A:A,C:C,"x")', 'SUMIFS(A:A,B1:B1048575,"a")',
    'COUNTIFS(A:A,">0",B:C,"a")', 'SUMIF(B3:C4,"a",A2:C9)', 'AVERAGEIF(B3:B6,"a",A2:B5)'
)

Write-Workbook (Join-Path $OutDir 'cfunc_m3_5/oracle.xlsx') @(
    @('criteria_semantics', (New-OracleSheet $criteriaInputs @($criteriaFormulas | ForEach-Object { Get-Stored $_ }))),
    @('mismatched_shapes', (New-OracleSheet $shapeInputs @($shapeFormulas | ForEach-Object { Get-Stored $_ })))
)

# --- dc_q9rb/oracle.xlsx ------------------------------------------------------------------------

# Sheet number_text_for_concatenation: the cells of dc_q9rb__number_text_for_concatenation's test
# sheet and its oracle rows' formulas in its order. Each number a row reads from a cell is in
# column A of the row's own line, written as the shortest decimal that reads back as it (up to 17
# significant digits, so it cannot be typed); a row whose number is a literal or a sum leaves
# column A blank. -0, the largest subnormal and the smallest subnormal of both signs are written
# as they are; the capture records the & text of those rows.
$numberTextInputs = @(
    (New-Number 'A1' '1'),
    (New-Number 'A2' '123456789012345'),
    (New-Number 'A3' '999999999999999'),
    (New-Number 'A4' '999999999999999.4'),
    (New-Number 'A5' '999999999999999.5'),
    (New-Number 'A6' '999999999999999.9'),
    (New-Number 'A8' '1000000000000001'),
    (New-Number 'A9' '1000000000000000.5'),
    (New-Number 'A10' '1234567890123456'),
    (New-Number 'A11' '1234567890123455'),
    (New-Number 'A12' '1234567890123465'),
    (New-Number 'A13' '9007199254740992'),
    (New-Number 'A15' '1E+19'),
    (New-Number 'A16' '99999999999999900000'),
    (New-Number 'A17' '9.999999999999999E+19'),
    (New-Number 'A19' '1.2345678901234568E+20'),
    (New-Number 'A21' '9.999999999999999E+20'),
    (New-Number 'A22' '1.2345678901234568E+21'),
    (New-Number 'A31' '0.000123456789012345'),
    (New-Number 'A32' '1.23456789012345E-05'),
    (New-Number 'A33' '1.2345678901234E-06'),
    (New-Number 'A38' '0.30000000000000004'),
    (New-Number 'A42' '0.9999999999999999'),
    (New-Number 'A44' '-0.5'),
    (New-Number 'A45' '-1000000000000000'),
    (New-Number 'A46' '-1.23456789012345E-05'),
    (New-Number 'A47' '-1.7976931348623157E+308'),
    (New-Number 'A49' '-0'),
    (New-Number 'A50' '1.7976931348623157E+308'),
    (New-Number 'A51' '2.2250738585072014E-308'),
    (New-Number 'A52' '2.225073858507201E-308'),
    (New-Number 'A53' '5E-324'),
    (New-Number 'A54' '-5E-324'),
    (New-Number 'A55' '1.23456789012345E+98'),
    (New-Number 'A56' '1.23456789012345E+99'),
    (New-Number 'A57' '1.23456789012345E+100'),
    (New-Number 'A58' '1.23456789012345E-99'),
    (New-Number 'A60' '1.8055722654199485E-306'),
    (New-Number 'A61' '7200547821627.375'),
    (New-Number 'A67' '-1E+19'),
    (New-Number 'A68' '-99999999999999900000'),
    (New-Number 'A69' '-9.999999999999999E+19'),
    (New-Number 'A70' '-0.000123456789012345'),
    (New-Number 'A72' '-1234567890123455'),
    (New-Number 'A73' '-7200547821627.375'),
    (New-Number 'A74' '-1.8055722654199485E-306'),
    (New-Number 'A75' '9.99999999999995E-99'),
    (New-Number 'A76' '-9.99999999999995E-99'),
    (New-Number 'A77' '521310271410497.5'),
    (New-Number 'A78' '5758361573115.375'),
    (New-Number 'A79' '-6278515587045.875'),
    (New-Number 'A80' '543212677140.0625'),
    (New-Number 'A81' '4037429.935952025'),
    (New-Number 'A82' '5.248519552626765E-06'),
    (New-Number 'A83' '-0.9198894815859745'),
    (New-Number 'A84' '-7.302438853328905E+163'),
    (New-Number 'A85' '23.61461297829835'),
    (New-Number 'A86' '80216.89735826345')
)
$numberTextFormulas = @(
    # Integers up to and past 15 significant digits, and the boundaries around 1E+15.
    'A1&""', 'A2&""', 'A3&""', 'A4&""', 'A5&""', 'A6&""', '1E+15&""', 'A8&""', 'A9&""',
    'A10&""', 'A11&""', 'A12&""', 'A13&""', '"x"&1E+16',
    # Where the text switches to E notation, around 1E+20 and 1E+21.
    'A15&""', 'A16&""', 'A17&""', '1E+20&""', 'A19&""', '1E+21&""', 'A21&""', 'A22&""',
    '1E+22&""',
    # Small numbers from 1E-4 to 1E-10, and past them.
    '0.0001&""', '0.00001&""', '0.000001&""', '1E-7&""', '1E-8&""', '1E-9&""', '1E-10&""',
    'A31&""', 'A32&""', 'A33&""', '1E-18&""', '1.5E-18&""', '1E-19&""',
    # Sums whose 16th and 17th digits differ from their 15-digit text.
    '0.1+0.2&""', 'A38&""', '1/3&""', '2/3&""', '1-1E-16&""', 'A42&""',
    # Negative numbers and negative zero.
    '-1&""', 'A44&""', 'A45&""', 'A46&""', 'A47&""', '-0&""', 'A49&""',
    # The largest and smallest normal and subnormal binary64 values.
    'A50&""', 'A51&""', 'A52&""', 'A53&""', 'A54&""',
    # Three-digit exponents, the exponent 99 of either sign, and a tie at the 16th digit.
    'A55&""', 'A56&""', 'A57&""', 'A58&""', '1E+100&""', 'A60&""', 'A61&""',
    # A number on either side, two numbers, and the "op"&cell form of a criterion.
    '"a"&1', '1&"a"', '"="&A2', 'A1&"-"', 'A2&A3',
    # Negative numbers whose magnitude's text takes 20 characters (the sign does not count), a
    # negative tie at the 16th digit and at the second rounding, and a second rounding that
    # carries into the exponent.
    'A67&""', 'A68&""', 'A69&""', 'A70&""', '-1E-18&""', 'A72&""', 'A73&""', 'A74&""',
    'A75&""', 'A76&""',
    # Exact ties at the 16th digit that Excel rounds away from zero; numbers within a relative
    # 2^-59 of such a tie; and two just beyond that distance.
    'A77&""', 'A78&""', 'A79&""', 'A80&""', 'A81&""', 'A82&""', 'A83&""', 'A84&""', 'A85&""',
    'A86&""'
)

Write-Workbook (Join-Path $OutDir 'dc_q9rb/oracle.xlsx') @(
    , @('number_text_for_concatenation', (New-OracleSheet $numberTextInputs $numberTextFormulas))
)

# --- cfunc_m3_6/oracle.xlsx ---------------------------------------------------------------------

# Sheet reference_result: the cells of cfunc_m3_6__reference_result's test sheet (A1:C3 hold 1 to 9,
# A4 text, B4 is blank, C4 TRUE; A10:A14 the keys k1 to k5 with numbers and text beside them, B13
# blank; A1000 the key kz and C1000 77), and its oracle rows' formulas in its order. XLOOKUP and
# XMATCH carry the _xlfn. prefix the file format gives functions newer than it.
$referenceInputs = @(
    (New-Number 'A1' '1'),
    (New-Number 'B1' '2'),
    (New-Number 'C1' '3'),
    (New-Number 'A2' '4'),
    (New-Number 'B2' '5'),
    (New-Number 'C2' '6'),
    (New-Number 'A3' '7'),
    (New-Number 'B3' '8'),
    (New-Number 'C3' '9'),
    (New-Text 'A4' 't'),
    (New-Logical 'C4' $true),
    (New-Text 'A10' 'k1'),
    (New-Number 'B10' '10'),
    (New-Text 'C10' 'x10'),
    (New-Text 'A11' 'k2'),
    (New-Number 'B11' '20'),
    (New-Text 'C11' 'x20'),
    (New-Text 'A12' 'k3'),
    (New-Number 'B12' '30'),
    (New-Text 'C12' 'x30'),
    (New-Text 'A13' 'k4'),
    (New-Text 'C13' 'x40'),
    (New-Text 'A14' 'k5'),
    (New-Number 'B14' '50'),
    (New-Text 'C14' 'x50'),
    (New-Text 'A1000' 'kz'),
    (New-Number 'C1000' '77')
)
$referenceFormulas = @(
    'INDEX(A1:C3,2,3)',
    'INDEX(A1:C3,2.7,1.2)',
    'INDEX(A1:A4,4)',
    'INDEX(A1:C1,3)',
    'INDEX(A1:C1,,2)',
    'INDEX(B1:B4,4)',
    'INDEX(A1:C3,0,2)',
    'INDEX(A1:C3,2,0)',
    'INDEX(A1:C3,0,0)',
    'INDEX((A1:A3,C1:C3),2,1,2)',
    'INDEX((A1:A3,C1:C3),3)',
    'INDEX(A:A,1000)',
    'INDEX(A1:C3,"2","3")',
    'INDEX(A1:C3,4,1)',
    'INDEX(A1:C3,1,1,2)',
    'INDEX(A1:C3,2)',
    'INDEX(A1:C3,-1,1)',
    'INDEX(A1:C3,#N/A,#DIV/0!)',
    'SUM(INDEX(A1:C3,0,2))',
    'SUM(INDEX(A1:C3,2,0))',
    'SUM(INDEX(A1:C3,0,0))',
    'ROWS(INDEX(A1:C3,0,2))',
    'COLUMNS(INDEX(A1:C3,2,0))',
    'AREAS(INDEX((A1:A3,C1:C3),1,1,2))',
    'ROW(INDEX(A1:C3,2,3))',
    'COLUMN(INDEX(A1:C3,2,3))',
    'COUNTBLANK(INDEX(A1:C4,4,0))',
    'INDEX(B:B,MATCH("k3",A:A,0))',
    'INDEX(B:B,MATCH("K5",A:A,0))',
    'INDEX(C:C,MATCH("kz",A:A,0))',
    'INDEX(B:B,MATCH("k4",A:A,0))',
    'INDEX(B:B,MATCH("k9",A:A,0))',
    '_xlfn.XLOOKUP("k3",A10:A14,B10:B14)',
    '_xlfn.XLOOKUP("k4",A10:A14,B10:B14)',
    '_xlfn.XLOOKUP("k9",A10:A14,B10:B14,C4)',
    'SUM(_xlfn.XLOOKUP("k2",A10:A14,B10:C14))',
    'CHOOSE(2,A1,B1)',
    'SUM(CHOOSE(2,A1:A3,B1:B3))',
    'ROWS(CHOOSE(1,A1:A3,B1:B2))',
    'CHOOSE(2,"a",B4)',
    'VLOOKUP("k3",A10:C14,3,FALSE)',
    'VLOOKUP("k4",A10:B14,2,FALSE)',
    'INDEX({1,2;3,4},2,1)',
    'INDEX({1,2;3,4},0,2)',
    'SUM(INDEX({1,2;3,4},2))',
    'INDEX(5,1)',
    'AREAS((A1:B2,C3,A4))',
    'ROWS(A:A)',
    'AREAS(INDEX(A1:C3,0,2))',
    'ROWS(INDEX({1,2;3,4},0,2))',
    'SUM(INDEX({1,2;3,4},0,2))',
    'VLOOKUP("k4",A10:B14,2,FALSE)&""',
    # INDEX's area_num, a logical, fractional, negative or huge index, column_num omitted on a line
    # of an array, and column_num given empty or omitted on a grid.
    'INDEX((A1:A3,C1:C3),1,1,0)',
    'INDEX((A1:A3,C1:C3),1,1,)',
    'INDEX({1,2;3,4},1,1,2)',
    'INDEX(5,1,1,2)',
    'INDEX(A1:C3,TRUE,1)',
    'INDEX(A1:C3,-0.5,1)',
    'INDEX(A1:C3,1E+300,1)',
    'INDEX({1,2,3},2)',
    'INDEX({1;2;3},2)',
    'INDEX(A1:C3,1.9999,1)',
    'SUM(INDEX(A1:C3,2,))',
    'SUM(INDEX(A1:C3,2))'
)

# Sheet lookup_edges: the cells of cfunc_m3_6__lookup_edges' test sheet (ascending, descending,
# text, wildcard, mixed-kind, table, unsorted, nearly equal and sweep lines; C9 is blank; the error
# pairs of F:G and P:Q), the source formulas of P:Q, and its oracle rows' formulas in its order. The
# workbook carries the metadata part the source formulas name.
$edgesInputs = @(
    (New-Number 'A1' '1'),
    (New-Text 'B1' 'apple'),
    (New-Number 'C1' '10'),
    (New-Number 'A2' '2'),
    (New-Text 'B2' 'Banana'),
    (New-Number 'C2' '20'),
    (New-Number 'A3' '2'),
    (New-Text 'B3' 'banana'),
    (New-Number 'C3' '30'),
    (New-Number 'A4' '2'),
    (New-Text 'B4' 'cherry'),
    (New-Number 'C4' '40'),
    (New-Number 'A5' '3'),
    (New-Text 'B5' 'date'),
    (New-Number 'C5' '50'),
    (New-Number 'A6' '5'),
    (New-Text 'B6' 'fig'),
    (New-Number 'C6' '60'),
    (New-Number 'A7' '5'),
    (New-Text 'B7' 'a*b'),
    (New-Number 'C7' '3'),
    (New-Number 'A8' '4'),
    (New-Text 'B8' 'a?c'),
    (New-Text 'C8' 'a'),
    (New-Number 'A9' '3'),
    (New-Text 'B9' '~'),
    (New-Number 'A10' '3'),
    (New-Text 'B10' 'x~y'),
    (New-Logical 'C10' $true),
    (New-Number 'A11' '2'),
    (New-Text 'B11' '1'),
    (New-Error 'C11' '#N/A'),
    (New-Number 'A12' '1'),
    (New-Number 'B12' '1'),
    (New-Number 'C12' '3'),
    (New-Number 'A13' '1'),
    (New-Number 'B13' '2'),
    (New-Number 'C13' '3'),
    (New-Text 'A14' 'p'),
    (New-Text 'B14' 'q'),
    (New-Text 'C14' 'r'),
    (New-Number 'A15' '10'),
    (New-Number 'B15' '20'),
    (New-Number 'C15' '30'),
    (New-Number 'A16' '3'),
    (New-Text 'B16' ''),
    (New-Number 'C16' '0.3'),
    (New-Number 'A17' '1'),
    (New-Text 'B17' 'ab'),
    (New-Number 'C17' '0.30000000000000004'),
    (New-Number 'A18' '4'),
    (New-Text 'B18' 'a'),
    (New-Number 'C18' '1'),
    (New-Number 'A19' '1'),
    (New-Text 'B19' 'a*c'),
    (New-Number 'C19' '1.0000000000000002'),
    (New-Number 'A20' '5'),
    (New-Text 'B20' 'abc'),
    (New-Number 'C20' '0.1'),
    (New-Text 'B21' 'c'),
    (New-Number 'C21' '4'),
    (New-Number 'A22' '1'),
    (New-Text 'B22' 'a'),
    (New-Number 'C22' '1'),
    (New-Text 'A23' 'd'),
    (New-Number 'B23' '5'),
    (New-Text 'C23' 'd'),
    (New-Text 'A24' 'a'),
    (New-Number 'B24' '2'),
    (New-Number 'C24' '1'),
    (New-Text 'A25' 'a'),
    (New-Number 'B25' '3'),
    (New-Number 'C25' '5'),
    (New-Number 'B26' '3'),
    (New-Logical 'C26' $true),
    (New-Number 'A27' '1'),
    (New-Number 'B27' '5'),
    (New-Number 'A28' '5'),
    (New-Number 'B28' '1'),
    (New-Number 'A29' '4'),
    (New-Number 'B29' '1'),
    (New-Logical 'A30' $false),
    (New-Error 'B30' '#N/A'),
    (New-Error 'C30' '#DIV/0!'),
    (New-Number 'A31' '2'),
    (New-Number 'B31' '1'),
    (New-Error 'C31' '#N/A'),
    (New-Number 'A32' '5'),
    (New-Number 'B32' '3'),
    (New-Error 'C32' '#NUM!'),
    (New-Number 'A33' '2'),
    (New-Logical 'C33' $true),
    (New-Text 'A34' 'd'),
    (New-Number 'C34' '7'),
    (New-Number 'A35' '5'),
    (New-Text 'A36' 'c'),
    (New-Number 'A37' '3'),
    (New-Number 'A40' '-1'),
    (New-Number 'A41' '0'),
    (New-Number 'A42' '1'),
    # H1:H4 is 1, 2, blank, 3; H6:H9 1, blank, 2, 3; H11:H14 3, blank, 2, 1.
    (New-Number 'H1' '1'),
    (New-Number 'H2' '2'),
    (New-Number 'H4' '3'),
    (New-Number 'H6' '1'),
    (New-Number 'H8' '2'),
    (New-Number 'H9' '3'),
    (New-Number 'H11' '3'),
    (New-Number 'H13' '2'),
    (New-Number 'H14' '1'),
    # I1:I3 is blank, empty text and 1; I5:J5 blank and 1; I7:J7 empty text and blank; I9:J9
    # blank and 20.
    (New-Text 'I2' ''),
    (New-Number 'I3' '1'),
    (New-Number 'J5' '1'),
    (New-Text 'I7' ''),
    (New-Number 'J9' '20'),
    # K1 and K2: two numbers that differ in their 16th significant digit.
    (New-Number 'K1' '1.234567890123455e-10'),
    (New-Number 'K2' '1.234567890123454e-10'),
    # L1:M4: a table whose first column holds 1, 2, a blank cell and 3, beside 10, 20, 99 and 30;
    # L6:O7 the same table across.
    (New-Number 'L1' '1'),
    (New-Number 'M1' '10'),
    (New-Number 'L2' '2'),
    (New-Number 'M2' '20'),
    (New-Number 'M3' '99'),
    (New-Number 'L4' '3'),
    (New-Number 'M4' '30'),
    (New-Number 'L6' '1'),
    (New-Number 'M6' '2'),
    (New-Number 'O6' '3'),
    (New-Number 'L7' '10'),
    (New-Number 'M7' '20'),
    (New-Number 'N7' '99'),
    (New-Number 'O7' '30')
)
# F1:G56: every ordered pair of two different errors of the eight a cell constant can hold, by
# first error and then second, in XMATCH's order of them.
$errorLiterals = @('#NULL!', '#DIV/0!', '#VALUE!', '#REF!', '#NAME?', '#NUM!', '#N/A', '#GETTING_DATA')
$errorPairs = @()
foreach ($first in $errorLiterals) {
    foreach ($second in $errorLiterals) {
        if ($first -ne $second) { $errorPairs += , @($first, $second) }
    }
}
for ($k = 1; $k -le $errorPairs.Count; $k++) {
    $edgesInputs += , (New-Error ('F' + $k) $errorPairs[$k - 1][0])
    $edgesInputs += , (New-Error ('G' + $k) $errorPairs[$k - 1][1])
}
# P1:Q54: every ordered pair of two different errors of the eleven XMATCH orders with #SPILL!,
# #FIELD! or #CALC! among them, by first error and then second. No cell constant or formula literal
# holds those three (Excel refuses a workbook that holds them as constants), so a cell of one of
# them holds its source formula, a dynamic-array formula over its one cell: FILTER with no match
# (#CALC!), SEQUENCE(1,2), whose spill the filled cell to its right blocks (#SPILL!; R holds the
# text that blocks one in Q, Q blocks one in P), and FIELDVALUE of a number (#FIELD!). The other
# cells hold the error as a constant.
$orderedErrors = @('#NULL!', '#DIV/0!', '#VALUE!', '#REF!', '#NAME?', '#NUM!', '#N/A', '#GETTING_DATA', '#SPILL!', '#FIELD!', '#CALC!')
$sourceFormulas = @{
    '#SPILL!' = '_xlfn.SEQUENCE(1,2)';
    '#FIELD!' = '_xlfn.FIELDVALUE(A1,"x")';
    '#CALC!' = '_xlfn._xlws.FILTER(A1:A6,A1:A6>100)'
}
$sourcePairs = @()
foreach ($first in $orderedErrors) {
    foreach ($second in $orderedErrors) {
        if ($first -ne $second -and ($sourceFormulas.ContainsKey($first) -or $sourceFormulas.ContainsKey($second))) {
            $sourcePairs += , @($first, $second)
        }
    }
}
$edgesSources = @()
for ($k = 1; $k -le $sourcePairs.Count; $k++) {
    foreach ($place in @(@('P', 0), @('Q', 1))) {
        $literal = $sourcePairs[$k - 1][$place[1]]
        $ref = $place[0] + $k
        if ($sourceFormulas.ContainsKey($literal)) {
            $edgesSources += , @($ref, $sourceFormulas[$literal])
        } else {
            $edgesInputs += , (New-Error $ref $literal)
        }
    }
    if ($sourcePairs[$k - 1][1] -eq '#SPILL!') { $edgesInputs += , (New-Text ('R' + $k) 'block') }
}
$edgesFormulas = @(
    'MATCH(2,A1:A6,0)',
    'MATCH(2,A1:A6,1)',
    'MATCH(2,A1:A6)',
    'MATCH(2.5,A1:A6,1)',
    'MATCH(0,A1:A6,1)',
    'MATCH(9,A1:A6,1)',
    'MATCH(3,A7:A12,-1)',
    'MATCH(3.5,A7:A12,-1)',
    'MATCH(6,A7:A12,-1)',
    'MATCH(0,A7:A12,-1)',
    'MATCH(3,A7:A12,1)',
    'MATCH(2,A1:A6,-1)',
    'MATCH("BANANA",B1:B6,0)',
    'MATCH("BANANA",B1:B6,1)',
    'MATCH("b*",B1:B6,0)',
    'MATCH("?pple",B1:B6,0)',
    'MATCH("c",B1:B6,1)',
    'MATCH("a",B1:B6,1)',
    'MATCH("a~*b",B7:B12,0)',
    'MATCH("x~y",B7:B12,0)',
    'MATCH("~~",B7:B12,0)',
    'MATCH("1",B7:B12,0)',
    'MATCH(1,B7:B12,0)',
    'MATCH("2",A1:A6,0)',
    'MATCH(TRUE,C7:C12,0)',
    'MATCH("*",C7:C12,0)',
    'MATCH(0,C7:C12,0)',
    'MATCH(4,C7:C12,1)',
    'MATCH("b",C7:C12,1)',
    'MATCH(2,A1:A6,2)',
    'MATCH(2,A1:A6,-0.5)',
    'MATCH(2,A1:A6,"0")',
    'MATCH(2,A1:A6,)',
    'MATCH(2,A1:B6,0)',
    'MATCH(2,2,0)',
    'MATCH(2,{1,2,3},0)',
    'MATCH(C17,C16:C20,0)',
    'MATCH(0.3,C17:C20,0)',
    'MATCH(C19,C18:C19,0)',
    ('MATCH("' + ('a' * 256) + '",B1:B6,0)'),
    'MATCH(#N/A,A1:A6,"x")',
    'MATCH(2,#REF!,#N/A)',
    'MATCH(,A40:A42,0)',
    'MATCH(C9,A40:A42,1)',
    'MATCH("c",A21:A37,1)',
    'MATCH("bb",B21:B32,1)',
    'MATCH(2.5,C21:C26,-1)',
    'MATCH(2.5,C21:C26,1)',
    '_xlfn.XMATCH(2,A1:A6)',
    '_xlfn.XMATCH(2,A1:A6,0,-1)',
    '_xlfn.XMATCH(2.5,A1:A6,-1)',
    '_xlfn.XMATCH(2.5,A1:A6,-1,-1)',
    '_xlfn.XMATCH(2.5,A1:A6,1)',
    '_xlfn.XMATCH(0,A1:A6,-1)',
    '_xlfn.XMATCH(9,A1:A6,1)',
    '_xlfn.XMATCH(2,A1:A6,0,2)',
    '_xlfn.XMATCH(3,A7:A12,0,-2)',
    '_xlfn.XMATCH(2.5,A1:A6,-1,2)',
    '_xlfn.XMATCH("b*",B1:B6,2)',
    '_xlfn.XMATCH("b*",B1:B6,0)',
    '_xlfn.XMATCH("banana",B1:B6,0,-1)',
    '_xlfn.XMATCH(2,A1:A6,0,0)',
    '_xlfn.XMATCH(2,A1:A6,-1.5)',
    '_xlfn.XMATCH(2.5,A1:A6,-0.5)',
    '_xlfn.XMATCH("b*",B1:B6,2,2)',
    '_xlfn.XMATCH(2,A1:B6)',
    '_xlfn.XMATCH(2,2)',
    '_xlfn.XMATCH("2",A1:A6)',
    '_xlfn.XMATCH(C9,C7:C12)',
    '_xlfn.XMATCH("",C7:C12)',
    '_xlfn.XMATCH(C9,A40:A42,-1)',
    '_xlfn.XMATCH(C9,A40:A42,1)',
    '_xlfn.XMATCH(6,C30:C34,1)',
    '_xlfn.XMATCH(8,C30:C34,1)',
    '_xlfn.XMATCH(8,C30:C32,1)',
    '_xlfn.XMATCH(2.5,C21:C26,1,-2)',
    '_xlfn.XMATCH(99,A40:A45,1)',
    '_xlfn.XMATCH(99,A40:A45,1,-1)',
    '_xlfn.XMATCH(2.5,C21:C26,-1)',
    '_xlfn.XMATCH(2.5,C21:C26,-1,-1)',
    '_xlfn.XLOOKUP(2,A1:A6,C1:C6)',
    '_xlfn.XLOOKUP(2,A1:A6,C1:C6,,0,-1)',
    '_xlfn.XLOOKUP(7,A1:A6,C1:C6)',
    '_xlfn.XLOOKUP(7,A1:A6,C1:C6,"none")',
    '_xlfn.XLOOKUP(7,A1:A6,C1:C6,)',
    '_xlfn.XLOOKUP(2.5,A1:A6,C1:C6,,-1)',
    '_xlfn.XLOOKUP(2.5,A1:A6,C1:C6,,1)',
    '_xlfn.XLOOKUP(3,A1:A6,C1:C5)',
    '_xlfn.XLOOKUP(#N/A,A1:A6,C1:C5)',
    '_xlfn.XLOOKUP(2,A13:C13,A15:C15)',
    '_xlfn.XLOOKUP(2,A13:C13,A1:A3)',
    '_xlfn.XLOOKUP(7,A1:A6,C1:C6,#DIV/0!)',
    '_xlfn.XLOOKUP("BANANA",B1:B6,C1:C6,,0,-1)',
    'VLOOKUP(3,A1:C6,2,FALSE)',
    'VLOOKUP(2,A1:C6,3)',
    'VLOOKUP(2.5,A1:C6,3,TRUE)',
    'VLOOKUP(0,A1:C6,2,TRUE)',
    'VLOOKUP(2.5,A1:C6,2,FALSE)',
    'VLOOKUP(2.5,A1:C6,2,)',
    'VLOOKUP(3,A1:C6,4,FALSE)',
    'VLOOKUP(3,A1:C6,0,FALSE)',
    'VLOOKUP(99,A1:C6,0,FALSE)',
    'VLOOKUP(3,A1:C6,2.9,FALSE)',
    'VLOOKUP(3,A1:C6,-2147483647,FALSE)',
    'VLOOKUP(3,A1:C6,-2147483648,FALSE)',
    'VLOOKUP("b*",B1:C6,2,FALSE)',
    'VLOOKUP("3",A1:C6,2,FALSE)',
    'VLOOKUP(3,A1:C6,2,"x")',
    'VLOOKUP(3,A1:C6,2,"TRUE")',
    'VLOOKUP(C9,A40:A42,1,FALSE)',
    'VLOOKUP(3,A9:C10,3,FALSE)',
    'VLOOKUP(2.5,C21:C26,1,TRUE)',
    'HLOOKUP(2,A13:C15,3,FALSE)',
    'HLOOKUP(2.5,A13:C15,2)',
    'HLOOKUP(3,A13:C15,4,FALSE)',
    'HLOOKUP("q",A14:C15,2,FALSE)',
    'LOOKUP(3,A1:A6,C1:C6)',
    'LOOKUP(2,A1:A6,C1:C6)',
    'LOOKUP(0,A1:A6,C1:C6)',
    'LOOKUP(9,A1:A6,C1:C6)',
    'LOOKUP(2,A13:C13,A15:C15)',
    'LOOKUP(2,A1:A3,A15:C15)',
    'LOOKUP(3,A1:C6)',
    'LOOKUP(2,A13:C15)',
    'LOOKUP("C",B1:B6,C1:C6)',
    'LOOKUP(C9,A40:A42)',
    'LOOKUP(3,{1,2,3},{"a","b"})',
    'LOOKUP(2,{1,#DIV/0!,1,#DIV/0!})',
    'CHOOSE(2,"a","b","c")',
    'CHOOSE(2.9,"a","b","c")',
    'CHOOSE(0,"a","b")',
    'CHOOSE(4,"a","b","c")',
    'CHOOSE("2","a","b")',
    'CHOOSE(TRUE,"a","b")',
    'CHOOSE(#N/A,"a","b")',
    'CHOOSE(2,"a",)',
    'CHOOSE(2,#DIV/0!,2)',
    'ROW(B3)',
    'ROW(B3:C3)',
    'COLUMN(C5)',
    'ROW(#REF!)',
    'ROWS(A1:C5)',
    'ROWS({1,2;3,4})',
    'ROWS(5)',
    'ROWS("a")',
    'ROWS(#N/A)',
    'ROWS((A1:A3,B1:B2))',
    'COLUMNS(A1:C5)',
    'AREAS(A1)',
    'ROW(B3:B5)',
    'MATCH("a-b",B1:B2,1)',
    'MATCH({1,2},A1:A6,0)',
    'VLOOKUP(3,A1:C6,2,"1")',
    'ROW((A1,B2))',
    'COLUMN((A1,B2))',
    'COLUMN(B3:D3)',
    '_xlfn.XLOOKUP(1,A1,B1:C2)',
    '_xlfn.XLOOKUP(9,A1,B1:C2,"nf")',
    '_xlfn.XLOOKUP(9,A1,A13:C13)',
    '_xlfn.XLOOKUP(1,A1,A13:C13)',
    '_xlfn.XLOOKUP(1,A1,C1:C3)',
    '_xlfn.XLOOKUP(1,A1,B1:C2,,#DIV/0!)',
    '_xlfn.XMATCH(2,A1:A6,0,C9)',
    '_xlfn.XMATCH(2,A1:A6,C9)',
    'INDEX(,1)',
    'INDEX(,1,#DIV/0!)',
    '_xlfn.XMATCH(1,)',
    '_xlfn.XLOOKUP(1,,C1:C3)',
    '_xlfn.XLOOKUP(1,A1:A3,)',
    '_xlfn.XLOOKUP(1,A1:A3,,,#DIV/0!)',
    'VLOOKUP(1,,1)',
    'VLOOKUP(1,,1,#DIV/0!)',
    'LOOKUP(1,)',
    'LOOKUP(1,A1:A6,)',
    'LOOKUP(1,#DIV/0!,)',
    'MATCH(1,,#DIV/0!)',
    'SUM(ROW(B3:B5))',
    'MATCH(4,ROW(B3:B5),0)',
    'SUM(COLUMN(B3:D3))',
    'ROW(CHOOSE(1,5))',
    'COLUMN(CHOOSE(1,"a"))',
    'AREAS(CHOOSE(1,5))',
    'ROW(CHOOSE(2,1,))',
    'AREAS(CHOOSE(2,1,))',
    'ROWS(CHOOSE(2,1,))',
    'COLUMNS(CHOOSE(2,1,))',
    'INDEX(CHOOSE(2,1,),1)',
    'ROWS(_xlfn.XLOOKUP(1,A1,A13:C13))',
    'COLUMNS(_xlfn.XLOOKUP(1,A1,A13:C13))',
    'SUM(_xlfn.XLOOKUP(1,A1,A13:C13))',
    'ROWS(_xlfn.XLOOKUP(1,A1,C1:C3))',
    'COLUMNS(_xlfn.XLOOKUP(1,A1,C1:C3))',
    'SUM(_xlfn.XLOOKUP(1,A1,C1:C3))',
    'VLOOKUP(3,A9:C10,3,FALSE)&""',
    'CHOOSE(2,"a",)&""',
    'MATCH(3,{1,3},0)',
    'MATCH(2.5,{1,2,3})',
    'MATCH(2,{1,2,2,2,3},1)',
    'MATCH(2,{1,2,2,2,3})',
    'MATCH(2,{1,2,2,2,3},)',
    'MATCH(2,{3,2,2,2,1},-1)',
    'MATCH(2.5,{1,3,2,4},1)',
    'MATCH(2.5,{3,1,4,2},-1)',
    'MATCH(C9,I5:J5,0)',
    'MATCH(9.99E+307,H1:H4,1)',
    'MATCH(2.5,H6:H9,1)',
    'MATCH(2.5,H11:H14,-1)',
    'MATCH(2.5,{1,#DIV/0!,2,3},1)',
    'MATCH(2.5,{1,#N/A,2,3},1)',
    'MATCH("a~*",{"abc","a*"},0)',
    'MATCH("Abc",{"abc","def"},0)',
    'MATCH(2,{1,2},0)',
    'MATCH(2,{1,2;3,4},0)',
    'MATCH(C17,C16,0)',
    'MATCH(K1,K2,0)',
    'MATCH({1,2,3},{2,4,6,8},0)',
    '_xlfn.XMATCH(2,{1,2,3})',
    '_xlfn.XMATCH(2,{2,1,2},0,-1)',
    '_xlfn.XMATCH(9,{1,2})',
    '_xlfn.XMATCH(C17,C16,0)',
    '_xlfn.XMATCH(K1,K2,0)',
    '_xlfn.XMATCH("Abc",{"abc","zzz"})',
    ('_xlfn.XMATCH("' + [char]0xC4 + 'bc",{"' + [char]0xE4 + 'BC","zzz"})'),
    '_xlfn.XMATCH(1,{"1"})',
    '_xlfn.XMATCH(1,{1},9)',
    '_xlfn.XMATCH(1,{1},,9)',
    '_xlfn.XMATCH("a*",{"zzz","abc"},2)',
    '_xlfn.XMATCH("a~*",{"abc","a*"},2)',
    '_xlfn.XMATCH("a~?",{"a1","a?"},2)',
    '_xlfn.XMATCH("a~~b",{"a~b","ab"},2)',
    '_xlfn.XMATCH("a*",{"abc","def","ade"},2,-1)',
    '_xlfn.XMATCH(3,{1,2,3},,2)',
    '_xlfn.XMATCH(3,{4,3,2},,-2)',
    '_xlfn.XMATCH(2,{1,2,2,2,3},,2)',
    '_xlfn.XMATCH(2.9,{1,2,2,2,3},-1,2)',
    '_xlfn.XMATCH(2,{3,2,2,2,1},,-2)',
    '_xlfn.XMATCH(2.5,{3,2,2,2,1},-1,-2)',
    '_xlfn.XMATCH(2,{3,1,4,2},0,2)',
    '_xlfn.XMATCH(2.5,{3,1,4,2},1,2)',
    '_xlfn.XMATCH(2.5,{3,1,4,2},-1,2)',
    '_xlfn.XMATCH(9.99E+307,H1:H4,-1,2)',
    '_xlfn.XMATCH(2.5,H6:H9,1,2)',
    '_xlfn.XMATCH(2,H6:H9,,2)',
    '_xlfn.XMATCH(2.5,H11:H14,-1,-2)',
    '_xlfn.XMATCH(2.5,{1,#DIV/0!,2,3},1,2)',
    '_xlfn.XMATCH(2.5,{1,2,3},1)',
    '_xlfn.XMATCH(2.5,{1,2,3},-1)',
    '_xlfn.XMATCH(2,{1,2,3},1)',
    '_xlfn.XMATCH(2,{1,2,3},-1)',
    '_xlfn.XMATCH("Beta",{"alpha","beta","gamma"},,2)',
    '_xlfn.XMATCH(TRUE,{FALSE,TRUE},,2)',
    '_xlfn.XMATCH(2,{1,2,3},,)',
    '_xlfn.XMATCH(C9,I1:I3,0)',
    '_xlfn.XMATCH(,I1:I3,0)',
    '_xlfn.XMATCH("",I1:I3,0)',
    '_xlfn.XMATCH(#VALUE!,{1})',
    '_xlfn.XMATCH(2,{#VALUE!})',
    '_xlfn.XMATCH(2,{#DIV/0!,2})',
    '_xlfn.XMATCH(9,{1,#DIV/0!,2})',
    '_xlfn.XMATCH(3,{3,4})',
    '_xlfn.XLOOKUP(2,{1,2},{"one","two"})',
    '_xlfn.XLOOKUP("a*",{"abc","ade"},{"first","last"},,2,-1)',
    '_xlfn.XLOOKUP(9,{1,2,3},{10,20,30},"nf")',
    '_xlfn.XLOOKUP(2,{1,2,3},{10,20,30},,0,)',
    '_xlfn.XLOOKUP(,I5:J5,{10,20})',
    '_xlfn.XLOOKUP(C9,I5:J5,{10,20})',
    '_xlfn.XLOOKUP("",I7:J7,{10,20})',
    '_xlfn.XLOOKUP(1,{1,2},I9:J9)',
    '_xlfn.XLOOKUP(2,{3,1,4,2},{30,10,40,20},,0,2)',
    '_xlfn.XLOOKUP(2,{1,2,3},B1:D1)',
    '_xlfn.XLOOKUP(2,{1;2;3},{10,20,30})',
    '_xlfn.XLOOKUP(2,{1;2;3},{10,11;20,21;30,31})',
    '_xlfn.XLOOKUP(2,{1,2,3},B2:D4)',
    '_xlfn.XLOOKUP({1,2,3},{2,4,6,8},{20,40,60,80})',
    '_xlfn.XLOOKUP({1,2;3,4},{2,4,6,8},{20,40,60,80},{"NF","ignored"})',
    '_xlfn.XLOOKUP({2,4},{2;4;6},{20,21;40,41;60,61})',
    '_xlfn.XLOOKUP({2,4},{2,4,6},{20,40,60;21,41,61})',
    'VLOOKUP(2,{1,10;2,20;3,30},2,FALSE)',
    'VLOOKUP(2.9,{1,10;2,20;3,30},2)',
    'HLOOKUP(2,{1,2,3;10,20,30},2,FALSE)',
    'HLOOKUP(2.9,{1,2,3;10,20,30},2)',
    'VLOOKUP(2.9,L1:M4,2)',
    'VLOOKUP(9.99E+307,L1:M4,2)',
    'HLOOKUP(2.9,L6:O7,2)',
    'HLOOKUP(9.99E+307,L6:O7,2)',
    'VLOOKUP(0.5,{1,10;2,20;3,30},2)',
    'HLOOKUP(0.5,{1,2,3;10,20,30},2)',
    'VLOOKUP(4,{1,10;2,20;3,30},2,FALSE)',
    'HLOOKUP(4,{1,2,3;10,20,30},2,FALSE)',
    'VLOOKUP(2,{1,10;2,20;3,30},1.9,FALSE)',
    'HLOOKUP(2,{1,2,3;10,20,30},1.9,FALSE)',
    'VLOOKUP(2,{1,10;2,20;3,30},3,FALSE)',
    'HLOOKUP(2,{1,2,3;10,20,30},3,FALSE)',
    'VLOOKUP(2,{1,10;2,20;3,30},0,FALSE)',
    'HLOOKUP(2,{1,2,3;10,20,30},0,FALSE)',
    'VLOOKUP(2,{1,10;2,20;3,30},-1,FALSE)',
    'HLOOKUP(2,{1,2,3;10,20,30},-1,FALSE)',
    'VLOOKUP("b*",{"abc",10;"bcd",20},2,FALSE)',
    'HLOOKUP("b*",{"abc","bcd";10,20},2,FALSE)',
    'VLOOKUP(TRUE,{TRUE,1;FALSE,2},2,FALSE)',
    'VLOOKUP({1,2,3},{2,20;4,40;6,60;8,80},2,FALSE)',
    'HLOOKUP({1,2,3},{2,4,6,8;20,40,60,80},2,FALSE)',
    'LOOKUP(2.9,{1,2,3},{10,20,30})',
    'LOOKUP(2.9,{1,10;2,20;3,30})',
    'LOOKUP(2.2,{1,2,3},{"a","b","c"})',
    'LOOKUP(2,{2,1,3},{20,10,30})',
    'LOOKUP(2,{1,2;3,4;5,6},{10,20,30})',
    'CHOOSE(2.9,A1,"picked",#DIV/0!)',
    'CHOOSE(0.9,10,20)',
    'CHOOSE(4,10,20,30)',
    'CHOOSE(1,C9,7)',
    'CHOOSE({1,2,3,4},"Alpha","Bravo","Charlie","Delta")',
    'CHOOSE({1,2,3},{1;2;3},{10;20;30},{100;200;300})',
    'MATCH("South",{"North","South","North"},0)',
    '_xlfn.XLOOKUP("South",{"North","South","North"},{100,200,300})',
    'ROWS(A1:C7)',
    'COLUMNS(A1:C7)',
    'INDEX(A1:C7,2,3)',
    'INDEX(A1:C7,8,1)',
    'AREAS(A1:C7)'
)
# XMATCH(TRUE,pair,1) over each error pair of F1:G56, then of P1:Q54.
for ($k = 1; $k -le $errorPairs.Count; $k++) {
    $edgesFormulas += , ('_xlfn.XMATCH(TRUE,F' + $k + ':G' + $k + ',1)')
}
for ($k = 1; $k -le $sourcePairs.Count; $k++) {
    $edgesFormulas += , ('_xlfn.XMATCH(TRUE,P' + $k + ':Q' + $k + ',1)')
}
$edgesFormulas += @(
    # A linear next-smaller or next-larger search decides by kind before it orders texts.
    '_xlfn.XMATCH(5,{"a b","c d",6},1)',
    '_xlfn.XMATCH(TRUE,{"a b","c d",FALSE},-1)',
    '_xlfn.XLOOKUP(5,{"a b","c d",6},{10,20,30},,1)',
    # The rules of the per-ID manifest that earlier rounds read from Excel ad hoc only.
    'MATCH(2,A1:A6,0.5)',
    'MATCH(2,A1:A6,C9)',
    'MATCH(2,(A1:A3,A4:A6),0)',
    'MATCH(1,,0)',
    'MATCH(2,{2},0)',
    'MATCH(1,A1,0)',
    '_xlfn.XMATCH(2.5,A1:A6,1.5)',
    '_xlfn.XMATCH(3,A7:A12,0,-1.5)',
    '_xlfn.XMATCH(2,(A1:A3,A4:A6))',
    ('_xlfn.XMATCH("' + ('a' * 256) + '",B1:B6)'),
    '_xlfn.XMATCH(1,,#DIV/0!)',
    '_xlfn.XMATCH(#N/A,)',
    '_xlfn.XLOOKUP(2,A1:B6,C1:C6)',
    '_xlfn.XLOOKUP(2,(A1:A3,A4:A6),C1:C6)',
    '_xlfn.XLOOKUP(2,A1:A6,(C1:C3,C4:C6))',
    'VLOOKUP(2.5,A1:C6,2,0)',
    'VLOOKUP(2.5,A1:C6,2,C9)',
    'VLOOKUP(2,A1:C6,3,FALSE)',
    'VLOOKUP(2.5,A1:C6,3,5)',
    'VLOOKUP(#N/A,A1:C6,#DIV/0!)',
    'VLOOKUP(2,(A1:B3,A4:B6),2,FALSE)',
    'VLOOKUP(2,2,1)',
    ('VLOOKUP("' + ('a' * 256) + '",B1:C6,2,FALSE)'),
    'HLOOKUP(1,,1)',
    'LOOKUP(2,A13:C14)',
    'LOOKUP(1,A1:A6,C1)',
    'LOOKUP(2,A1:A3,A13:C15)',
    'LOOKUP(3,{1,2,3},5)',
    'LOOKUP(2,(A1:A3,A4:A6),C1:C6)',
    'LOOKUP(2,A1:A6,(C1:C3,C4:C6))',
    ('LOOKUP("' + ('a' * 256) + '",B1:B6)'),
    'LOOKUP(1,{1,2},I9:J9)',
    'LOOKUP(0,A1:A6,)',
    'CHOOSE(FALSE,"a","b")',
    'CHOOSE(C9,"a","b")',
    'COLUMNS({1,2,3})',
    'COLUMNS(5)',
    'COLUMNS(#N/A)',
    'COLUMNS((A1:A3,B1:B2))'
)

Write-Workbook (Join-Path $OutDir 'cfunc_m3_6/oracle.xlsx') @(
    @('reference_result', (New-OracleSheet $referenceInputs $referenceFormulas)),
    @('lookup_edges', (New-OracleSheet $edgesInputs $edgesFormulas $edgesSources))
) -DynamicArrays

# --- dc_e00t/oracle.xlsx ------------------------------------------------------------------------

# Sheet digit_labels_as_text: the cells of dc_e00t__digit_labels_as_text's test sheet (A1:A8
# labels holding digits beside the number 2016 and the text 2016, B1:B8 the powers of two 1 to
# 128, C1:C10 criteria and texts: two labels, a date, a time, an amount in R, a label with an en
# dash, a label of 314 characters, the text 2016-17 and the fractions 1 1/2 and 1/2), and its
# oracle rows' formulas in its order: the hand-written rows, then each word of $labelWords in each
# form of $labelForms, as "text"+0, the word in place of the underscore.
# Sheet value_readings: in column D, VALUE("text") of each of those generated texts whose word is
# one of $valueSampleWords, in the same order, Excel's second reading of them. calc_func has no
# VALUE, so no row computes these cells; dc_e00t__excel_comparison checks them against the texts'
# "text"+0 rows.
$enDash = [string][char]0x2013
$labelInputs = @(
    (New-Text 'A1' '2016-17 Winter'),
    (New-Text 'A2' '2047-48 Summer'),
    (New-Text 'A3' '2047-48 Winter'),
    (New-Text 'A4' 'Q1 2016'),
    (New-Number 'A5' '2016'),
    (New-Text 'A6' '2016'),
    (New-Text 'A7' '2016-17 winter'),
    (New-Text 'A8' '2063-2064 Winter'),
    (New-Number 'B1' '1'),
    (New-Number 'B2' '2'),
    (New-Number 'B3' '4'),
    (New-Number 'B4' '8'),
    (New-Number 'B5' '16'),
    (New-Number 'B6' '32'),
    (New-Number 'B7' '64'),
    (New-Number 'B8' '128'),
    (New-Text 'C1' '2016-17 Winter'),
    (New-Text 'C2' 'Q1 2016'),
    (New-Text 'C3' '1 Jan 2020'),
    (New-Text 'C4' '10:30 am'),
    (New-Text 'C5' 'R1'),
    (New-Text 'C6' ('2016' + $enDash + '17 Winter')),
    (New-Text 'C7' ((@('2016-17 Winter') * 21) -join ' ')),
    (New-Text 'C8' '2016-17'),
    (New-Text 'C9' '1 1/2'),
    (New-Text 'C10' '1/2')
)
$labelFormulas = New-Object System.Collections.ArrayList
foreach ($formula in @(
    # Labels read as numbers, and IF, SUM and COUNT over a label.
    'A1+0', '0+A4', 'A2*2', '-A3', 'A8%', 'A1^1', 'C7+0', 'IF(A1,1,2)', 'SUM("Q1 2016",1)',
    'COUNT("Q1 2016",1)',
    # Text written like a date or a time that holds a word no number text holds.
    '"2020-01-02T10:30"+0', '"10:30 a.m."+0', '"1st January 2020"+0', '"10:30Z"+0',
    # Comparisons over labels.
    'A1=A7', 'A1=C1', 'A1<>A3', 'A4=2016', 'A1>A5',
    # Criteria over labels.
    'COUNTIF(A1:A8,"2016-17 Winter")', 'COUNTIF(A1:A8,"="&C1)', 'COUNTIF(A1:A8,C1)',
    'COUNTIF(A1:A8,"<>"&C1)', 'COUNTIF(A1:A8,"Q1 2016")', 'COUNTIF(A1:A8,C2)', 'COUNTIF(A1:A8,2016)',
    'COUNTIF(A1:A8,"=2016")', 'SUMIF(A1:A8,"="&C1,B1:B8)', 'SUMIF(A1:A8,2016,B1:B8)',
    'SUMIFS(B1:B8,A1:A8,"="&C1)', 'SUMIFS(B1:B8,A1:A8,"="&A2)',
    'SUMIFS(B1:B8,A1:A8,"<>"&C1,A1:A8,"<>"&A2)', 'AVERAGEIF(A1:A8,"="&C1,B1:B8)',
    'AVERAGEIFS(B1:B8,A1:A8,"="&C1)', 'COUNTIFS(A1:A8,"="&C1,B1:B8,">1")',
    'MAXIFS(B1:B8,A1:A8,"="&C1)', 'MINIFS(B1:B8,A1:A8,"="&C1)',
    # Forms the rule leaves undecided.
    'C3+0', 'C4+0', 'C5+0', 'C6+0', 'C8+0', 'COUNTIF(A1:A8,"="&C3)', 'COUNTIF(A1:A8,C8)',
    'COUNTIF(C3:C8,2016)', 'IF(C3,1,2)', 'C9+0', 'C10+0', 'COUNTIF(A1:A8,C9)', 'COUNTIF(A1:A8,C10)',
    'SUMIFS(B1:B8,A1:A8,"1/2")',
    # Ranges of different shapes beside an undecided criterion.
    'SUMIFS(B1:B8,A1:A7,"1/2")'
)) { [void]$labelFormulas.Add((Get-Stored $formula)) }
$labelWords = @(
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'R', 'AM', 'am', 'pm',
    'ja', 'fe', 'ma', 'ap', 'ju', 'au', 'se', 'oc', 'no', 'de',
    'jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec', 'sept', 'JAN',
    'january', 'february', 'march', 'april', 'june', 'july', 'august', 'september', 'october',
    'november', 'december',
    'janx', 'market', 'decade',
    'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday',
    'mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun',
    'winter', 'summer', 'fy', 'noon', 'gmt', 'st'
)
# A date with a month name, a time, an exponent, a currency symbol, numeric dates with / and -,
# digits grouped by this machine's thousands separator (a space), a parenthesised negative, a
# percentage, a currency amount, a weekday's long date and a fraction, each with the word.
$labelForms = @(
    '1 _ 2020', '10:30 _', '1_5', '_1', '2020/1/2 _', '2020-1-2 _', '1 000 _', '(1) _', '1% _',
    'R1 _', '_, 1 January 2020', '1 1/2 _'
)
# A word no number text holds, a month name's prefix and a time marker.
$valueSampleWords = @('winter', 'jan', 'pm')
$valueFormulas = New-Object System.Collections.ArrayList
foreach ($word in $labelWords) {
    foreach ($form in $labelForms) {
        $text = $form.Replace('_', $word)
        [void]$labelFormulas.Add('"' + $text + '"+0')
        if ($valueSampleWords -ccontains $word) { [void]$valueFormulas.Add('VALUE("' + $text + '")') }
    }
}

Write-Workbook (Join-Path $OutDir 'dc_e00t/oracle.xlsx') @(
    @('digit_labels_as_text', (New-OracleSheet $labelInputs $labelFormulas)),
    @('value_readings', (New-OracleSheet @() $valueFormulas))
)

# --- dc_fwpz/oracle.xlsx ------------------------------------------------------------------------

# XOR and IFNA came after the file format's first version, so a file writes them with the _xlfn.
# prefix, without which Excel does not recognize them. No formula of this sheet calls a function
# whose name ends in XOR or IFNA, such as BITXOR.
function Get-LogicalStored([string]$Formula) {
    return $Formula.Replace('XOR(', '_xlfn.XOR(').Replace('IFNA(', '_xlfn.IFNA(')
}

# TRUE spelled in fullwidth letters (U+FF34 U+FF32 U+FF35 U+FF25).
$fullwidthTrue = [string][char]0xFF34 + [string][char]0xFF32 + [string][char]0xFF35 + [string][char]0xFF25

# Sheet logical_functions: the cells of dc_fwpz__logical_functions' test sheet (A1:A10 one value
# of each kind AND, OR and XOR tell apart, A7 blank and A8 empty text; B1:B4 two errors and two
# logicals; A12:B13 a number, a logical and two errors, one in each corner off the diagonal; C1:C4
# text and a blank cell, no logical, and C1048576 TRUE, the only logical of column C), and its
# oracle rows' formulas in its order.
$logicalInputs = @(
    (New-Logical 'A1' $true),
    (New-Logical 'A2' $false),
    (New-Number 'A3' '1'),
    (New-Number 'A4' '0'),
    (New-Text 'A5' 'TRUE'),
    (New-Text 'A6' 'x'),
    (New-Text 'A8' ''),
    (New-Number 'A9' '-2.5'),
    (New-Text 'A10' 'FALSE'),
    (New-Number 'A12' '1'),
    (New-Error 'A13' '#REF!'),
    (New-Error 'B1' '#DIV/0!'),
    (New-Error 'B2' '#N/A'),
    (New-Logical 'B3' $true),
    (New-Logical 'B4' $false),
    (New-Error 'B12' '#NUM!'),
    (New-Logical 'B13' $true),
    (New-Text 'C1' 'abc'),
    (New-Text 'C3' 'TRUE'),
    (New-Text 'C4' '1'),
    (New-Logical 'C1048576' $true)
)
# 255 arguments, the most AND and OR take (their pages), and 254, the most XOR takes (Microsoft's XOR
# page gives 1 to 254 conditions).
$manyTrue = (@('TRUE') * 255) -join ','
$manyFalse = (@('FALSE') * 255) -join ','
$xorTrue = (@('TRUE') * 254) -join ','
$logicalFormulas = @(
    'TRUE()', 'FALSE()',
    'AND(TRUE,TRUE)', 'AND(TRUE,FALSE)', 'AND(1,2)', 'AND(1,0)', 'AND(-2.5)', 'AND("TRUE",TRUE)',
    'AND("false",TRUE)', 'AND(TRUE,"x")', 'AND("x")', 'AND(TRUE,"1")', 'AND(TRUE,"0")',
    'AND(TRUE,"")', 'AND(TRUE,)', 'AND(,)', 'AND(FALSE,#N/A)', 'AND(#N/A,#DIV/0!)',
    'AND(#DIV/0!,#N/A)', 'AND("x",#N/A)', 'AND(A1:A10)', 'AND(A3,A9)', 'AND(A5:A8)', 'AND(C1:C4)',
    'AND(C:C)', 'AND(A1,A5)', 'AND(A10,TRUE)', 'AND({TRUE,FALSE})', 'AND({1,2,"x"})',
    'AND({"TRUE"})', 'AND(B1:B4)', 'AND(B3:B4,B2)', 'AND(B2,#DIV/0!)', 'AND(#DIV/0!,B2)',
    'AND({TRUE,#N/A},#DIV/0!)', 'AND({TRUE,#DIV/0!},B2)', 'AND(B2,{TRUE,#DIV/0!})',
    'AND(B2,"x",#DIV/0!)', 'AND((A1:A2,B3:B4))', 'AND({TRUE,#N/A,#DIV/0!})',
    'AND({TRUE,#N/A;#DIV/0!,TRUE})', 'AND(FALSE,{TRUE,#N/A})', 'AND(A7)', ('AND(' + $manyTrue + ')'),
    'OR(FALSE,FALSE)', 'OR(FALSE,TRUE)', 'OR(0,0)', 'OR(0,-2.5)', 'OR(FALSE,"TRUE")', 'OR(FALSE,"x")',
    'OR(FALSE,"1")', 'OR(FALSE,"")', 'OR(FALSE," TRUE")', ('OR(FALSE,"' + $fullwidthTrue + '")'),
    'OR(FALSE,"#N/A")', 'OR("x","y")', 'OR(FALSE,)', 'OR(,)', 'OR(TRUE,#N/A)', 'OR(B1,#N/A)',
    'OR(A1:A10)', 'OR(A2,A4)', 'OR(A5:A8)', 'OR(C:C)', 'OR({FALSE,0,"TRUE"})', 'OR(B3:B4,B1)',
    'OR(A6,FALSE)', 'OR(A12:B13)', ('OR(' + $manyFalse + ')'),
    'XOR(TRUE,FALSE)', 'XOR(TRUE,TRUE)', 'XOR(TRUE,TRUE,TRUE)', 'XOR(FALSE,FALSE)', 'XOR(1,2)',
    'XOR(A1:A10)', 'XOR(A5:A8)', 'XOR({TRUE,TRUE},TRUE)', 'XOR(TRUE,"x")', 'XOR("x")',
    'XOR(TRUE,#N/A)', 'XOR(B1,B2)', 'XOR(B2,#DIV/0!)', 'XOR(C:C)', 'XOR(TRUE,)',
    ('XOR(' + $xorTrue + ')'),
    'NOT(TRUE)', 'NOT(FALSE)', 'NOT(0)', 'NOT(-2.5)', 'NOT("TRUE")', 'NOT("false")', 'NOT("x")',
    'NOT("")', 'NOT(#N/A)', 'NOT(A7)', 'NOT(A5)', 'NOT(A6)', 'NOT(A8)', 'NOT(B1)', 'NOT(A3)',
    'NOT({TRUE,FALSE})', 'NOT("1")',
    # Digit text alone, and two arrays.
    'AND("1")', 'AND({FALSE,TRUE,TRUE},{TRUE,TRUE,TRUE})',
    # A7's blank value given directly: IFNA and IFERROR take values and return a blank cell's value
    # as it is.
    'AND(TRUE,IFNA(A7,0))', 'AND(IFNA(A7,0))', 'AND(TRUE,IFERROR(A7,FALSE))',
    'OR(FALSE,IFERROR(A7,FALSE))', 'OR(IFERROR(A7,FALSE))', 'OR(IFNA(A7,0))',
    'XOR(TRUE,IFNA(A7,0))', 'XOR(IFNA(A7,0))', 'XOR(IFERROR(A7,FALSE))',
    'NOT(IFNA(A7,0))', 'NOT(IFERROR(A7,FALSE))'
)

Write-Workbook (Join-Path $OutDir 'dc_fwpz/oracle.xlsx') @(
    , @('logical_functions', (New-OracleSheet $logicalInputs @($logicalFormulas | ForEach-Object { Get-LogicalStored $_ })))
)

# --- cdoc_jpha/oracle.xlsx ----------------------------------------------------------------------

# Sheet text_and_number_comparison: the rows of cdoc_jpha__text_and_number_comparison, read from
# cdoc_jpha/rows.txt beside this script, whose header states its line kinds: the formulas in its
# order, the two numbers of each N line in columns A and B of the first of its four rows, and the
# constants of its C lines in columns C and E.

# A text of rows.txt: \XXXX is the UTF-16 code unit XXXX, \{N} repeats the code unit after it N
# times, and any other character is itself.
function Get-JphaText([string]$Spec) {
    $text = New-Object System.Text.StringBuilder
    $i = 0
    while ($i -lt $Spec.Length) {
        $count = 1
        if ($Spec[$i] -ceq [char]'\' -and $i + 1 -lt $Spec.Length -and $Spec[$i + 1] -ceq [char]'{') {
            $close = $Spec.IndexOf('}', $i)
            $count = [int]$Spec.Substring($i + 2, $close - $i - 2)
            $i = $close + 1
        }
        if ($Spec[$i] -ceq [char]'\') {
            $unit = [char][Convert]::ToInt32($Spec.Substring($i + 1, 4), 16)
            $i += 5
        } else {
            $unit = $Spec[$i]
            $i += 1
        }
        [void]$text.Append($unit, $count)
    }
    return $text.ToString()
}

# Whether a formula writes the character with this code inside a text literal: printable ASCII and
# every character from U+00A0 on, except the no-break, soft hyphen and other spaces and invisible
# characters and the combining diacritical marks, which it writes through UNICHAR.
function Test-JphaLiteral([int]$Code) {
    if ($Code -ge 0x20 -and $Code -le 0x7E) { return $true }
    if ($Code -lt 0xA0) { return $false }
    if (@(0xA0, 0xAD, 0x2002, 0x2003, 0x2007, 0x2009, 0x202F, 0x2060, 0xFEFF) -contains $Code) { return $false }
    if ($Code -ge 0x200B -and $Code -le 0x200F) { return $false }
    if ($Code -ge 0x300 -and $Code -le 0x36F) { return $false }
    return $true
}

# A text as a formula writes it: each run of literal characters as a text literal, each other
# character as CHAR (below U+0080) or UNICHAR of its code, joined by & in parentheses when there is
# more than one part.
function Get-JphaExpression([string]$Text) {
    $parts = New-Object System.Collections.ArrayList
    $run = New-Object System.Text.StringBuilder
    foreach ($unit in $Text.ToCharArray()) {
        $code = [int]$unit
        if (Test-JphaLiteral $code) {
            [void]$run.Append($unit)
        } else {
            if ($run.Length -gt 0) {
                [void]$parts.Add('"' + $run.ToString().Replace('"', '""') + '"')
                [void]$run.Clear()
            }
            if ($code -lt 0x80) { [void]$parts.Add("CHAR($code)") } else { [void]$parts.Add("UNICHAR($code)") }
        }
    }
    if ($run.Length -gt 0 -or $parts.Count -eq 0) { [void]$parts.Add('"' + $run.ToString().Replace('"', '""') + '"') }
    if ($parts.Count -eq 1) { return [string]$parts[0] }
    return '(' + ($parts -join '&') + ')'
}

$jphaInputs = New-Object System.Collections.ArrayList
$jphaFormulas = New-Object System.Collections.ArrayList
foreach ($line in [System.IO.File]::ReadAllLines((Join-Path $PSScriptRoot 'cdoc_jpha/rows.txt'))) {
    if ($line.Length -eq 0 -or $line.StartsWith('#')) { continue }
    $f = $line.Split('|')
    switch -CaseSensitive ($f[0]) {
        't' {
            [void]$jphaFormulas.Add((Get-JphaExpression (Get-JphaText $f[2])) + $f[1] + (Get-JphaExpression (Get-JphaText $f[3])))
        }
        'f' { [void]$jphaFormulas.Add($f[2] + $f[1] + $f[3]) }
        'N' {
            $row = $jphaFormulas.Count + 1
            [void]$jphaInputs.Add((New-Number "A$row" $f[1]))
            [void]$jphaInputs.Add((New-Number "B$row" $f[2]))
            foreach ($formula in @("A$row=B$row", "A$row<B$row", "+A$row", "+B$row")) { [void]$jphaFormulas.Add($formula) }
        }
        'C' {
            if ($f[2].StartsWith('n:')) {
                [void]$jphaInputs.Add((New-Number $f[1] $f[2].Substring(2)))
            } else {
                [void]$jphaInputs.Add((New-Text $f[1] (Get-JphaText $f[2])))
            }
        }
        'c' { [void]$jphaFormulas.Add($f[2] + $f[1] + $f[3]) }
        'COUNTIF' {
            $criterion = $f[2].Substring(2)
            if ($f[2].StartsWith('t:')) { $criterion = Get-JphaExpression (Get-JphaText $criterion) }
            [void]$jphaFormulas.Add('COUNTIF(' + $f[1] + ',' + $criterion + ')')
        }
        'L' {
            $key = $f[2].Substring(2)
            if ($f[2].StartsWith('t:')) { $key = Get-JphaExpression (Get-JphaText $key) }
            $array = $f[3]
            if ($array.StartsWith('{')) {
                $literal = Get-JphaExpression (Get-JphaText $array.Substring(1, $array.Length - 2))
                if (-not $literal.StartsWith('"')) { throw "cdoc_jpha/rows.txt: an array constant holds a character no literal holds: $line" }
                $array = '{' + $literal + '}'
            }
            $call = $f[1] + '(' + $key + ',' + $array
            if ($f[4].Length -gt 0) { $call += ',' + $f[4] }
            [void]$jphaFormulas.Add($call + ')')
        }
        default { throw "cdoc_jpha/rows.txt: a line of no known kind: $line" }
    }
}

# UNICHAR, XMATCH and XLOOKUP came after the file format's first version, so a file writes them
# with the _xlfn. prefix, without which Excel does not recognize them.
$jphaStored = @($jphaFormulas | ForEach-Object { $_.Replace('UNICHAR(', '_xlfn.UNICHAR(').Replace('XMATCH(', '_xlfn.XMATCH(').Replace('XLOOKUP(', '_xlfn.XLOOKUP(') })
Write-Workbook (Join-Path $OutDir 'cdoc_jpha/oracle.xlsx') @(
    , @('text_and_number_comparison', (New-OracleSheet $jphaInputs $jphaStored))
)
