# The generator of every workbook fixture under platform/calc-oracle-excel/tests/fixtures/ except
# infra_m1_2/untrusted-macro.xlsm, which the owner authors in Excel (tests/acceptance.rs says how).
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/infra_m1_2/{replay,untrusted-link,link-target}.xlsx and
# <directory>/worker/{macro-sheet.xlsm,external-content.xlsx,pivot-refresh.xlsx,
# query-table-refresh.xlsx,external-functions.xlsx,dynamic-array.xlsx,spill.xlsx,
# spill-saved-cm.xlsx,spill-saved-values.xlsx}. Run with this directory as -OutDir it rewrites the
# committed fixtures; the unit test fixtures_are_the_output_of_their_generator (src/worker_tests.rs)
# runs it into a fresh directory and compares every file with the committed one byte for byte.
#
# Each package is a minimal SpreadsheetML package written as text: every part is UTF-8 XML, stored
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
$SheetType = 'application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml'
$StylesType = 'application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml'
$WorkbookType = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml'

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
        $data = [System.Text.Encoding]::UTF8.GetBytes($part[1])
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

# A worksheet part: $Rows is an array of rows, each an array of cell XML strings; $After is XML
# placed after sheetData (conditional formatting, data validation, table parts).
function New-Worksheet($Rows, [string]$After = '') {
    $text = $Decl + '<worksheet xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><sheetData>'
    $number = 0
    foreach ($row in $Rows) {
        $number += 1
        $text += '<row r="' + $number + '">' + ($row -join '') + '</row>'
    }
    return $text + '</sheetData>' + $After + '</worksheet>'
}

function New-Number([string]$Ref, $Value) { return '<c r="' + $Ref + '"><v>' + $Value + '</v></c>' }

function New-Formula([string]$Ref, [string]$Formula, $Cached = $null) {
    $value = ''
    if ($null -ne $Cached) { $value = '<v>' + $Cached + '</v>' }
    return '<c r="' + $Ref + '"><f>' + (Get-Escaped $Formula) + '</f>' + $value + '</c>'
}

function New-Relationships($Relationships) {
    $text = $Decl + '<Relationships xmlns="' + $PkgRel + '">'
    foreach ($r in $Relationships) {
        $mode = ''
        if ($r.Count -gt 3) { $mode = ' TargetMode="' + $r[3] + '"' }
        $text += '<Relationship Id="' + $r[0] + '" Type="' + $r[1] + '" Target="' + $r[2] + '"' + $mode + '/>'
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

# Writes a package. $Spec holds:
#   sheets       an array of @(name, state or '', worksheet XML, sheet relationships or $null)
#   sheetsExtra  further sheet elements of the workbook (a macro sheet's), after the worksheets
#   workbookType the main part's content type
#   extraRels    workbook relationships beyond sheets and styles: @(type, target), numbered after them
#   inWorkbook   XML placed in the workbook after sheets: @(before calcPr, after calcPr)
#   parts        further parts: @(name, text, content type or '')
function Write-Workbook([string]$Path, $Spec) {
    $overrides = New-Object System.Collections.ArrayList
    [void]$overrides.Add(@('/xl/workbook.xml', $Spec.workbookType))
    [void]$overrides.Add(@('/xl/styles.xml', $StylesType))
    $parts = New-Object System.Collections.ArrayList
    $rels = New-Object System.Collections.ArrayList
    $entries = ''
    $index = 0
    foreach ($sheet in $Spec.sheets) {
        $index += 1
        $part = "worksheets/sheet$index.xml"
        [void]$rels.Add(@("rId$index", "$Rel/worksheet", $part))
        $state = ''
        if ($sheet[1] -ne '') { $state = ' state="' + $sheet[1] + '"' }
        $entries += '<sheet name="' + $sheet[0] + '" sheetId="' + $index + '"' + $state + ' r:id="rId' + $index + '"/>'
        [void]$overrides.Add(@("/xl/$part", $SheetType))
        [void]$parts.Add(@("xl/$part", $sheet[2]))
        if ($null -ne $sheet[3]) { [void]$parts.Add(@("xl/worksheets/_rels/sheet$index.xml.rels", (New-Relationships $sheet[3]))) }
    }
    $next = $index + 1
    [void]$rels.Add(@("rId$next", "$Rel/styles", 'styles.xml'))
    foreach ($extra in $Spec.extraRels) {
        $next += 1
        [void]$rels.Add(@("rId$next", $extra[0], $extra[1]))
    }
    foreach ($part in $Spec.parts) {
        [void]$parts.Add(@($part[0], $part[1]))
        if ($part[2] -ne '') { [void]$overrides.Add(@('/' + $part[0], $part[2])) }
    }
    $workbook = $Decl + '<workbook xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><bookViews><workbookView/></bookViews>' +
        '<sheets>' + $entries + $Spec.sheetsExtra + '</sheets>' + $Spec.inWorkbook[0] + '<calcPr fullCalcOnLoad="1"/>' + $Spec.inWorkbook[1] + '</workbook>'
    $types = $Decl + '<Types xmlns="' + $ContentTypes + '">' +
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
        '<Default Extension="xml" ContentType="application/xml"/>'
    foreach ($o in $overrides) { $types += '<Override PartName="' + $o[0] + '" ContentType="' + $o[1] + '"/>' }
    $types += '</Types>'
    $all = New-Object System.Collections.ArrayList
    [void]$all.Add(@('[Content_Types].xml', $types))
    [void]$all.Add(@('_rels/.rels', (New-Relationships @(, @('rId1', "$Rel/officeDocument", 'xl/workbook.xml')))))
    [void]$all.Add(@('xl/workbook.xml', $workbook))
    [void]$all.Add(@('xl/_rels/workbook.xml.rels', (New-Relationships $rels)))
    [void]$all.Add(@('xl/styles.xml', $Styles))
    foreach ($part in $parts) { [void]$all.Add($part) }
    Write-Package $Path $all
}

function New-Spec($Sheets) {
    return @{ sheets = $Sheets; sheetsExtra = ''; workbookType = $WorkbookType; extraRels = @(); inWorkbook = @('', ''); parts = @() }
}

# --- the fixtures -------------------------------------------------------------------------------

$fixtures = @{}

# infra_m1_2/replay.xlsx: two sheets (one hidden), numbers, text, a logical, an error, a shared group
# C1:C3 and a cross-sheet reference. No formula cell carries a cache: Excel calculates them.
$sheet1 = New-Worksheet @(
    @((New-Number 'A1' 1), (New-Formula 'B1' 'A1+A2'), '<c r="C1"><f t="shared" ref="C1:C3" si="0">A1*2</f></c>', (New-Formula 'D1' 'SUM(A1:A3)')),
    @((New-Number 'A2' 2), (New-Formula 'B2' '0.1+0.2'), '<c r="C2"><f t="shared" si="0"/></c>', (New-Formula 'D2' 'Hidden!A1+1')),
    @((New-Number 'A3' 3), (New-Formula 'B3' '1/0'), '<c r="C3"><f t="shared" si="0"/></c>'),
    @(, (New-Formula 'B4' '"ab"&"c"')),
    @(, (New-Formula 'B5' 'A1<A2'))
)
$hidden = New-Worksheet @(@(, (New-Formula 'A1' 'PI()')), @(, (New-Formula 'A2' 'SQRT(2)')))
$fixtures['infra_m1_2/replay.xlsx'] = New-Spec @(@('Sheet1', '', $sheet1, $null), @('Hidden', 'hidden', $hidden, $null))

# infra_m1_2/untrusted-link.xlsx: Sheet1!A1 reads Sheet1!A1 of the external workbook
# link-target.xlsx, whose cached value here is 111 while the file itself holds 333; B1 is A1+1.
# Unrefreshed, A1 is 111 and B1 112.
$spec = New-Spec @(, @('Sheet1', '', (New-Worksheet @(, @((New-Formula 'A1' '[1]Sheet1!$A$1' 111), (New-Formula 'B1' 'A1+1' 112)))), $null))
$spec.extraRels = @(, @("$Rel/externalLink", 'externalLinks/externalLink1.xml'))
$spec.inWorkbook = @('<externalReferences><externalReference r:id="rId3"/></externalReferences>', '')
$spec.parts = @(
    @('xl/externalLinks/externalLink1.xml', ($Decl + '<externalLink xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><externalBook r:id="rId1"><sheetNames><sheetName val="Sheet1"/></sheetNames><sheetDataSet><sheetData sheetId="0"><row r="1"><cell r="A1"><v>111</v></cell></row></sheetData></sheetDataSet></externalBook></externalLink>'), 'application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml'),
    @('xl/externalLinks/_rels/externalLink1.xml.rels', (New-Relationships @(, @('rId1', "$Rel/externalLinkPath", 'link-target.xlsx', 'External'))), '')
)
$fixtures['infra_m1_2/untrusted-link.xlsx'] = $spec

# infra_m1_2/link-target.xlsx: the external workbook, Sheet1!A1 = 333.
$fixtures['infra_m1_2/link-target.xlsx'] = New-Spec @(, @('Sheet1', '', (New-Worksheet @(, @(, (New-Number 'A1' 333)))), $null))

# worker/macro-sheet.xlsm: Sheet1, a worksheet with A1 =1+1 (cached 2), and Macro1, an Excel 4
# macro sheet whose relationship type is in Microsoft's namespace.
$spec = New-Spec @(, @('Sheet1', '', (New-Worksheet @(, @(, (New-Formula 'A1' '1+1' 2)))), $null))
$spec.workbookType = 'application/vnd.ms-excel.sheet.macroEnabled.main+xml'
$spec.extraRels = @(, @('http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet', 'macrosheets/sheet1.xml'))
# The macro sheet is the workbook's second sheet; rId3 is its relationship (after Sheet1 and styles).
$spec.sheetsExtra = '<sheet name="Macro1" sheetId="2" r:id="rId3"/>'
$spec.parts = @(, @('xl/macrosheets/sheet1.xml', ($Decl + '<xm:macrosheet xmlns="' + $Main + '" xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main"><sheetData><row r="1"><c r="A1"><f>RETURN()</f></c></row></sheetData></xm:macrosheet>'), 'application/vnd.ms-excel.macrosheet+xml'))
$fixtures['worker/macro-sheet.xlsm'] = $spec

# worker/external-content.xlsx: Sheet1 with A1 =1+1 (cached 2); a connections part with Refreshing
# (refreshOnLoad 1) and Quiet (no refreshOnLoad); a DDE link (externalLink1) and a link to another
# workbook (externalLink2).
$spec = New-Spec @(, @('Sheet1', '', (New-Worksheet @(, @(, (New-Formula 'A1' '1+1' 2)))), $null))
$spec.extraRels = @(
    @("$Rel/connections", 'connections.xml'),
    @("$Rel/externalLink", 'externalLinks/externalLink1.xml'),
    @("$Rel/externalLink", 'externalLinks/externalLink2.xml')
)
$spec.inWorkbook = @('<externalReferences><externalReference r:id="rId4"/><externalReference r:id="rId5"/></externalReferences>', '')
$spec.parts = @(
    @('xl/connections.xml', ($Decl + '<connections xmlns="' + $Main + '">' +
        '<connection id="1" name="Refreshing" type="1" refreshedVersion="6" refreshOnLoad="1" background="1" saveData="1"><dbPr connection="Provider=Microsoft.ACE.OLEDB.12.0;Data Source=C:\nowhere\data.accdb" command="T" commandType="3"/></connection>' +
        '<connection id="2" name="Quiet" type="1" refreshedVersion="6" background="1" saveData="1"><dbPr connection="Provider=Microsoft.ACE.OLEDB.12.0;Data Source=C:\nowhere\data.accdb" command="U" commandType="3"/></connection>' +
        '</connections>'), 'application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml'),
    @('xl/externalLinks/externalLink1.xml', ($Decl + '<externalLink xmlns="' + $Main + '"><ddeLink ddeService="nowhere-service" ddeTopic="nowhere-topic"><ddeItems><ddeItem name="item" advise="1"/></ddeItems></ddeLink></externalLink>'), 'application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml'),
    @('xl/externalLinks/externalLink2.xml', ($Decl + '<externalLink xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><externalBook r:id="rId1"><sheetNames><sheetName val="Sheet1"/></sheetNames></externalBook></externalLink>'), 'application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml'),
    @('xl/externalLinks/_rels/externalLink2.xml.rels', (New-Relationships @(, @('rId1', "$Rel/externalLinkPath", 'other.xlsx', 'External'))), '')
)
$fixtures['worker/external-content.xlsx'] = $spec

# worker/pivot-refresh.xlsx: two pivot caches over Sheet1!A1:A2, the first refreshing on load, the
# second not; the second is also kept at a part name the workbook's convention does not use, since
# the package reader finds pivot caches by their root element, not by their names.
$spec = New-Spec @(, @('Sheet1', '', (New-Worksheet @(@(, '<c r="A1" t="inlineStr"><is><t>N</t></is></c>'), @(, (New-Number 'A2' 1)))), $null))
$spec.extraRels = @(
    @("$Rel/pivotCacheDefinition", 'pivotCache/pivotCacheDefinition1.xml'),
    @("$Rel/pivotCacheDefinition", 'pivotCache/quiet-cache.xml')
)
$spec.inWorkbook = @('', '<pivotCaches><pivotCache cacheId="1" r:id="rId3"/><pivotCache cacheId="2" r:id="rId4"/></pivotCaches>')
$cacheBody = '<cacheSource type="worksheet"><worksheetSource ref="A1:A2" sheet="Sheet1"/></cacheSource><cacheFields count="1"><cacheField name="N" numFmtId="0"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1" containsInteger="1" minValue="1" maxValue="1"/></cacheField></cacheFields></pivotCacheDefinition>'
$spec.parts = @(
    @('xl/pivotCache/pivotCacheDefinition1.xml', ($Decl + '<pivotCacheDefinition xmlns="' + $Main + '" xmlns:r="' + $Rel + '" refreshOnLoad="1" refreshedVersion="6" recordCount="0">' + $cacheBody), 'application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml'),
    @('xl/pivotCache/quiet-cache.xml', ($Decl + '<pivotCacheDefinition xmlns="' + $Main + '" xmlns:r="' + $Rel + '" refreshOnLoad="0" refreshedVersion="6" recordCount="0">' + $cacheBody), 'application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml')
)
$fixtures['worker/pivot-refresh.xlsx'] = $spec

# worker/query-table-refresh.xlsx: a web query table on Sheet1 (through table1) that refreshes on
# load, over a data connection that does not: only the query table's own flag refuses it.
$spec = New-Spec @(, @('Sheet1', '', (New-Worksheet @(@(, '<c r="A1" t="inlineStr"><is><t>V</t></is></c>'), @(, (New-Number 'A2' 1))) '<tableParts count="1"><tablePart r:id="rId1"/></tableParts>'), @(, @('rId1', "$Rel/table", '../tables/table1.xml'))))
$spec.extraRels = @(, @("$Rel/connections", 'connections.xml'))
$spec.parts = @(
    @('xl/connections.xml', ($Decl + '<connections xmlns="' + $Main + '"><connection id="1" name="WebQuery" type="4" refreshedVersion="6" background="1" saveData="1"><webPr url="http://example.invalid/data" htmlTables="1"/></connection></connections>'), 'application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml'),
    @('xl/tables/table1.xml', ($Decl + '<table xmlns="' + $Main + '" id="1" name="ExternalData_1" displayName="ExternalData_1" ref="A1:A2" tableType="queryTable" totalsRowShown="0"><autoFilter ref="A1:A2"/><tableColumns count="1"><tableColumn id="1" uniqueName="1" name="V" queryTableFieldId="1"/></tableColumns><tableStyleInfo showFirstColumn="0" showLastColumn="0" showRowStripes="1" showColumnStripes="0"/></table>'), 'application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml'),
    @('xl/tables/_rels/table1.xml.rels', (New-Relationships @(, @('rId1', "$Rel/queryTable", '../queryTables/queryTable1.xml'))), ''),
    @('xl/queryTables/queryTable1.xml', ($Decl + '<queryTable xmlns="' + $Main + '" name="ExternalData_1" connectionId="1" refreshOnLoad="1" autoFormatId="16" applyNumberFormats="0" applyBorderFormats="0" applyFontFormats="0" applyPatternFormats="0" applyAlignmentFormats="0" applyWidthHeightFormats="0"><queryTableRefresh nextId="2"><queryTableFields count="1"><queryTableField id="1" name="V" tableColumnId="1"/></queryTableFields></queryTableRefresh></queryTable>'), 'application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml')
)
$fixtures['worker/query-table-refresh.xlsx'] = $spec

# worker/external-functions.xlsx: formulas that call functions the worker lists as reaching
# external data (in cells, with and without the _xlfn. prefix and in lower case, in a conditional
# formatting rule, a data validation rule and a defined name), and formulas that only look like
# such calls (a name in a string literal, longer names that end in one) and must not be refused.
$cells = @(
    @(, (New-Formula 'A1' 'RTD("progid.server",,"topic")')),
    @(, (New-Formula 'A2' '_xlfn.FILTERXML(_xlfn.WEBSERVICE("http://example.invalid/x"),"//a")')),
    @(, (New-Formula 'A3' '_xlfn.IMAGE("http://example.invalid/i.png")')),
    @(, (New-Formula 'A4' '_xlfn.STOCKHISTORY("MSFT",45000)')),
    @(, (New-Formula 'A5' 'cubevalue("Conn","[M].[X]")')),
    @(, (New-Formula 'A6' '"RTD(x)"&"IMAGE("&LEN("WEBSERVICE(")')),
    @(, (New-Formula 'A7' 'MYRTD(1)+SUM(2)')),
    @(, (New-Formula 'A8' 'SUM(1,2)' 3))
)
$after = '<conditionalFormatting sqref="B1"><cfRule type="expression" priority="1"><formula>RTD("progid.server",,"cf")&gt;0</formula></cfRule></conditionalFormatting>' +
    '<dataValidations count="1"><dataValidation type="custom" allowBlank="1" sqref="B2"><formula1>LEN(_xlfn.WEBSERVICE("http://example.invalid/dv"))&gt;0</formula1></dataValidation></dataValidations>'
$spec = New-Spec @(, @('Sheet1', '', (New-Worksheet $cells $after), $null))
$spec.inWorkbook = @('<definedNames><definedName name="Feed">_xlfn.WEBSERVICE("http://example.invalid/feed")</definedName><definedName name="Plain">Sheet1!$A$8</definedName></definedNames>', '')
$fixtures['worker/external-functions.xlsx'] = $spec

# worker/dynamic-array.xlsx: Sheet1!A1:A2 = 1 and 2, B1 =SUM(A1:A2*2) as Excel with dynamic arrays
# saves it (t="array" over B1 alone, with cell metadata cm="1" naming the dynamic-array property of
# the metadata part), and C1 =A1+1, an ordinary formula.
$spec = New-Spec @(, @('Sheet1', '', (New-Worksheet @(
    @((New-Number 'A1' 1), '<c r="B1" cm="1"><f t="array" ref="B1">SUM(A1:A2*2)</f><v>6</v></c>', (New-Formula 'C1' 'A1+1' 2)),
    @(, (New-Number 'A2' 2))
)), $null))
# The metadata part whose cell metadata 1 (cm="1") names the dynamic-array property XLDAPR.
$DynamicArrayMetadata = @('xl/metadata.xml', ($Decl + '<metadata xmlns="' + $Main + '" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray">' +
    '<metadataTypes count="1"><metadataType name="XLDAPR" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1" cellMeta="1"/></metadataTypes>' +
    '<futureMetadata name="XLDAPR" count="1"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk></futureMetadata>' +
    '<cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata></metadata>'), 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml')
$spec.extraRels = @(, @("$Rel/sheetMetadata", 'metadata.xml'))
$spec.parts = @(, $DynamicArrayMetadata)
$fixtures['worker/dynamic-array.xlsx'] = $spec

# worker/spill.xlsx: Sheet1!A1:A2 = 1 and 2 (constants); B1 =SEQUENCE(3,2) as Excel with dynamic
# arrays saves a formula that spills (t="array" over its spill range B1:C3, with cell metadata
# cm="1" naming the dynamic-array property XLDAPR), C1, B2, C2, B3 and C3 holding the values of its
# spill (2 to 6); D1 =A1+1, an ordinary formula; and E1 =SUM(A1:A2*2), a dynamic-array formula over
# E1 alone. The two packages after it are what the worker's tests have their stand-in Excel save in
# place of Excel; each differs from spill.xlsx in B1 only (New-SpillSpec's $B1).
function New-SpillSpec([string]$B1) {
    $spec = New-Spec @(, @('Sheet1', '', (New-Worksheet @(
        @((New-Number 'A1' 1), $B1, (New-Number 'C1' 2), (New-Formula 'D1' 'A1+1' 2), '<c r="E1" cm="1"><f t="array" ref="E1">SUM(A1:A2*2)</f><v>6</v></c>'),
        @((New-Number 'A2' 2), (New-Number 'B2' 3), (New-Number 'C2' 4)),
        @((New-Number 'B3' 5), (New-Number 'C3' 6))
    )), $null))
    $spec.extraRels = @(, @("$Rel/sheetMetadata", 'metadata.xml'))
    $spec.parts = @(, $DynamicArrayMetadata)
    return $spec
}
$fixtures['worker/spill.xlsx'] = New-SpillSpec '<c r="B1" cm="1"><f t="array" ref="B1:C3">_xlfn.SEQUENCE(3,2)</f><v>1</v></c>'
# worker/spill-saved-cm.xlsx: B1 over the same range with cell metadata cm="2", which names no block
# of the metadata part's cell metadata (it holds one), so not the dynamic-array property.
$fixtures['worker/spill-saved-cm.xlsx'] = New-SpillSpec '<c r="B1" cm="2"><f t="array" ref="B1:C3">_xlfn.SEQUENCE(3,2)</f><v>1</v></c>'
# worker/spill-saved-values.xlsx: B1 holds the value 1 and no formula.
$fixtures['worker/spill-saved-values.xlsx'] = New-SpillSpec (New-Number 'B1' 1)

foreach ($name in ($fixtures.Keys | Sort-Object)) {
    Write-Workbook (Join-Path $OutDir $name) $fixtures[$name]
}
