# The generator of the workbook fixtures under crates/calc-func/tests/fixtures/.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/cfunc_m1_2/oracle.xlsx, <directory>/cfunc_m3_2/oracle.xlsx,
# <directory>/cfunc_m3_4/oracle.xlsx, <directory>/cfunc_m3_5/oracle.xlsx and
# <directory>/dc_q9rb/oracle.xlsx. Run with this directory as -OutDir it rewrites the committed
# fixtures; the acceptance cases cfunc_m1_2__mixed_operands, cfunc_m3_2__rounding_boundaries,
# cfunc_m3_2__large_integers, cfunc_m3_4__argument_classes, cfunc_m3_5__criteria_semantics and
# dc_q9rb__number_text_for_concatenation run it into a fresh directory on Windows and compare its
# output with the committed workbooks byte for byte.
#
# cfunc_m1_2/oracle.xlsx is the workbook the owner's Excel capture of cfunc-m1.2 reads
# (`cargo xtask capture --fixture crates/calc-func/tests/fixtures/cfunc_m1_2/oracle.xlsx ...`),
# cfunc_m3_2/oracle.xlsx the one an Excel capture of cfunc-m3.2's rounding and integer rows reads,
# cfunc_m3_4/oracle.xlsx the one an Excel capture of cfunc-m3.4's aggregate rows reads,
# cfunc_m3_5/oracle.xlsx the one an Excel capture of cfunc-m3.5's conditional rows reads, and
# dc_q9rb/oracle.xlsx the one an Excel capture of dc-q9rb's number-text rows reads. Each has one
# sheet per case, named after the case: the cells the case's rows read, as the case's test sheet
# holds them, and in column D one formula per oracle row, from row 1, in the order the case runs
# its rows. Each case checks that its sheet holds exactly those formulas and cells. No formula cell
# carries a cache: Excel calculates them.
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

# A worksheet part: the constant cells $Inputs (each @(reference, XML), in columns A to C) and the
# formulas $Formulas, one per row of column D from row 1. Rows are written in order, each row's
# cells in column order.
function New-OracleSheet($Inputs, $Formulas) {
    $rows = @{}
    foreach ($cell in $Inputs) {
        $place = Split-Ref $cell[0]
        if ('ABC'.IndexOf($place[1]) -lt 0 -or $place[1].Length -ne 1) { throw "input cell $($cell[0]) is outside columns A to C" }
        if (-not $rows.ContainsKey($place[0])) { $rows[$place[0]] = @{} }
        $rows[$place[0]][$place[1]] = $cell[1]
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
        foreach ($column in @('A', 'B', 'C', 'D')) {
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

# Writes a workbook of the worksheets $Sheets (each @(name, worksheet XML)), in order.
function Write-Workbook([string]$Path, $Sheets) {
    $types = $Decl + '<Types xmlns="' + $ContentTypes + '">' +
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
        '<Default Extension="xml" ContentType="application/xml"/>' +
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>' +
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
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
    $types += '</Types>'
    $workbook = $Decl + '<workbook xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><bookViews><workbookView/></bookViews>' +
        '<sheets>' + $entries + '</sheets><calcPr fullCalcOnLoad="1"/></workbook>'
    $all = New-Object System.Collections.ArrayList
    [void]$all.Add(@('[Content_Types].xml', $types))
    [void]$all.Add(@('_rels/.rels', (New-Relationships @(, @('rId1', "$Rel/officeDocument", 'xl/workbook.xml')))))
    [void]$all.Add(@('xl/workbook.xml', $workbook))
    [void]$all.Add(@('xl/_rels/workbook.xml.rels', (New-Relationships $rels)))
    [void]$all.Add(@('xl/styles.xml', $Styles))
    foreach ($part in $parts) { [void]$all.Add($part) }
    Write-Package $Path $all
}

# --- cfunc_m1_2/oracle.xlsx ---------------------------------------------------------------------

# e with acute, small and capital. (PowerShell variable names ignore case, so the names differ.)
$eSmall = [string][char]0xE9
$eCapital = [string][char]0xC9

# Sheet mixed_operands: the cells of cfunc_m1_2__mixed_operands' test sheet (A5 and A6 blank; C2
# holds a text of 32767 characters, the longest a cell holds, and C1 one character less; C3 and C4
# hold two numbers of 14 significant digits that differ in the last), and its oracle rows'
# formulas in its order, the rows whose behaviour another bead owns included.
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
    (New-Number 'C4' '123456789012.33')
)
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
    '1E308*10', '0*-1', '1/0', 'A5/A6', '2.2250738585072014E-308/2', '"2"^2', 'TRUE^2', '2^3',
    '10^-2', '2^0.5', '3^1', '-0', '-"2"', '-TRUE', '-A5', '-""', '-2', '+"a"', '+TRUE', '+A5',
    '50%', '"50"%', 'TRUE%', '7%', '35%', '57%',
    '0.5-0.4-0.1', '1*(0.5-0.4-0.1)', '1.333+1.225-1.333-1.225', '1E15+1-1E15', '0.1+0.2-0.3',
    '1+1E-15-1', 'C3-C4',
    # Comparison; text collation and nearly equal numbers are cdoc-jpha's.
    '"1"=1', 'A1<>2', '1<"a"', '"a"<TRUE', 'FALSE>1E308', '"a"="A"', 'A5=0', 'A5=""',
    'A5=FALSE', 'A5=A6', 'A5<1', '"abc"<"abd"', '"B">"a"', '"a1"<"aa"', '"10"<"9"', '""<"a"',
    '"a"<="A"', '1>=1', '1<=0', 'TRUE>FALSE', ('"' + $eSmall + '"="' + $eSmall + '"'), '1=2',
    '0.1+0.2=0.3', ('"' + $eSmall + '"="' + $eCapital + '"'), '"a-b"<"ab"',
    '"apple pie"<"banana"', '"a"<"B"', '"A"="a"', '1+1E-15=1',
    # Concatenation, a number's text included (dc-q9rb measures it).
    '"a"&"b"', '"a"&TRUE', '"a"&FALSE', 'A5&"x"', 'A2&A4', '"a"&1', '1&"a"', 'C1&"b"',
    'C2&"b"',
    # SUM.
    'SUM("2",TRUE)', 'SUM(A1:A3)', 'SUM(A1:A3,"2",TRUE)', 'SUM({1,"2",TRUE})', 'SUM("a")',
    'SUM(A4)', 'SUM(A5)', 'SUM("")', 'SUM(1,)', 'SUM(1E308,1E308)', 'SUM(1E308,1E308,-1E308)',
    'SUM(B1:B3)', 'SUM((A3,A7))', 'SUM(A:A)',
    # IF; IF lifted over an array is cfml-m3.4's.
    'IF("TRUE",1,2)', 'IF("false",1,2)', 'IF("x",1,2)', 'IF("",1,2)', 'IF("1",1,2)',
    'IF(A5,1,2)', 'IF(0,1)', 'IF(2,"a",1/0)', 'IF(TRUE,,2)', 'IF(FALSE,1,)', 'IF(,1,2)',
    'IF({TRUE,FALSE},1,2)'
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
    'SUM(A1:B2,#NULL!)', 'SUM(#REF!,A1:B2)', 'SUM(1E308,1E308,#N/A)',
    'IF(#N/A,#DIV/0!,#REF!)', 'IF(TRUE,#DIV/0!,#N/A)', 'IF(FALSE,#DIV/0!,#N/A)',
    'IF("x",#DIV/0!,#N/A)'
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
