# The generator of the workbook fixtures under crates/calc-func/tests/fixtures/.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/cfunc_m1_2/oracle.xlsx and <directory>/cfunc_m3_4/oracle.xlsx. Run with this
# directory as -OutDir it rewrites the committed fixtures; the acceptance cases
# cfunc_m1_2__mixed_operands and cfunc_m3_4__argument_classes run it into a fresh directory on
# Windows and compare its output with the committed workbooks byte for byte.
#
# cfunc_m1_2/oracle.xlsx is the workbook the owner's Excel capture of cfunc-m1.2 reads
# (`cargo xtask capture --fixture crates/calc-func/tests/fixtures/cfunc_m1_2/oracle.xlsx ...`), and
# cfunc_m3_4/oracle.xlsx the one an Excel capture of cfunc-m3.4's aggregate rows reads. Each has one
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
    # Concatenation; a number's General text is cfml-m2.6's.
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
