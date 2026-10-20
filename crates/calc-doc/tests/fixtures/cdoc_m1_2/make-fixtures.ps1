# The generator of the cdoc-m1.2 package fixtures in this directory:
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/cells.xlsx, <directory>/extent.xlsx, <directory>/corner.xlsx and
# <directory>/manifest.json. Run with this directory as -OutDir it rewrites the committed fixtures.
#
# manifest.json is the generator's own record of what it wrote, computed here and not by the code
# under test: for each package its sheets in workbook order (name, sheetId, state, the r:id of the
# workbook's relationship to the sheet, the sheet's part name, and the CRC-32 and size of the part's
# content), and for each sheet every cell that holds a value or a formula, in row-major order, with
# what the cell holds as the generator chose it:
#
# - a number: the text written in <v> and the IEEE 754 bits .NET's Double.Parse gives that text
#   (each text is one .NET Framework parses exactly);
# - a text: its UTF-16 code units, as the string the generator chose before writing it as a shared
#   string item or an inline string (XML escapes, CDATA, rich-text runs, phonetic runs and the
#   _xHHHH_ escapes of ST_Xstring are how it is written, not what it is);
# - a logical or an error constant;
# - a formula: its text as a cell holding it reads (a shared formula's member included: the text
#   the generator states for that cell, which the file does not hold), its shared formula group
#   (si, master, ref) when it has one, the value written in <v> as the formula's cached result
#   (or none), and, for each formula whose cache was written deliberately wrong, the value the
#   formula has on this workbook as the generator states it (`correct`). Of the formulas written
#   with a cache, only F5's and G5's caches are right.
#
# Each cell's style index is recorded, and so are the cells written with a style and no value
# (styled_empty) and the <c> elements written with neither (unmaterialized). Each package's record
# also states the date system its workbook part names (date_system: 1900, as no package writes a
# workbookPr) and the number of definedName elements the part holds (defined_names).
#
# cells.xlsx holds two worksheets. Their workbook order (Cells, then Second Sheet) differs from
# their sheetId order (3, then 1) and from their part names (Cells is xl/worksheets/sheet2.xml).
# extent.xlsx holds one sheet whose only cell is XFD1048576; corner.xlsx is the same package with
# its only cell at A1.
#
# Part payloads are deflated by .NET's DeflateStream (System.IO.Compression, CompressionLevel
# Optimal), so the compressed bytes depend on the .NET Framework that runs the script; the CRC-32
# and size the manifest records are of the uncompressed content and do not. The script is ASCII
# and runs under Windows PowerShell 5.1.

param(
    [Parameter(Mandatory = $true)][string]$OutDir
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
if (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path (Get-Location).Path $OutDir }
if (-not (Test-Path -LiteralPath $OutDir)) { [void](New-Item -ItemType Directory -Path $OutDir) }

$Utf8 = New-Object System.Text.UTF8Encoding($false)
$Invariant = [System.Globalization.CultureInfo]::InvariantCulture
$Main = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'
$Rel = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
$PkgRel = 'http://schemas.openxmlformats.org/package/2006/relationships'
$ContentTypesNs = 'http://schemas.openxmlformats.org/package/2006/content-types'
$Decl = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' + "`n"

# --- CRC-32, deflate, the ZIP writer ------------------------------------------------------------

$CrcTable = New-Object 'int64[]' 256
for ($n = 0; $n -lt 256; $n++) {
    [int64]$c = $n
    for ($k = 0; $k -lt 8; $k++) {
        if ($c -band 1) { $c = 3988292384 -bxor ($c -shr 1) } else { $c = $c -shr 1 }
    }
    $CrcTable[$n] = $c
}

function Get-Crc32([byte[]]$Bytes) {
    [int64]$crc = 4294967295
    foreach ($b in $Bytes) { $crc = $CrcTable[($crc -bxor $b) -band 255] -bxor ($crc -shr 8) }
    return [uint32]($crc -bxor 4294967295)
}

function Get-Deflated([byte[]]$Content) {
    $memory = New-Object System.IO.MemoryStream
    $deflate = New-Object System.IO.Compression.DeflateStream($memory, [System.IO.Compression.CompressionLevel]::Optimal, $true)
    $deflate.Write($Content, 0, $Content.Length)
    $deflate.Dispose()
    return , $memory.ToArray()
}

# Writes the entries (each @(name, content)) deflated, sizes in the local headers, the central
# directory in the same order. Returns the archive's length.
function Write-Zip([string]$Path, $Entries) {
    $stream = New-Object System.IO.MemoryStream
    $w = New-Object System.IO.BinaryWriter($stream)
    $written = @()
    foreach ($e in $Entries) {
        $name = $Utf8.GetBytes($e[0])
        $content = [byte[]]$e[1]
        $payload = Get-Deflated $content
        $crc = Get-Crc32 $content
        $offset = [uint32]$stream.Position
        $w.Write([uint32]0x04034b50)
        $w.Write([uint16]20); $w.Write([uint16]0); $w.Write([uint16]8)
        $w.Write([uint16]0); $w.Write([uint16]0x0021)
        $w.Write([uint32]$crc); $w.Write([uint32]$payload.Length); $w.Write([uint32]$content.Length)
        $w.Write([uint16]$name.Length); $w.Write([uint16]0)
        $w.Write($name)
        $w.Write([byte[]]$payload)
        $written += , @($name, $crc, $payload.Length, $content.Length, $offset)
    }
    $start = [uint32]$stream.Position
    foreach ($x in $written) {
        $w.Write([uint32]0x02014b50)
        $w.Write([uint16]20); $w.Write([uint16]20); $w.Write([uint16]0); $w.Write([uint16]8)
        $w.Write([uint16]0); $w.Write([uint16]0x0021)
        $w.Write([uint32]$x[1]); $w.Write([uint32]$x[2]); $w.Write([uint32]$x[3])
        $w.Write([uint16]$x[0].Length); $w.Write([uint16]0); $w.Write([uint16]0)
        $w.Write([uint16]0); $w.Write([uint16]0); $w.Write([uint32]0)
        $w.Write([uint32]$x[4])
        $w.Write([byte[]]$x[0])
    }
    $size = [uint32]$stream.Position - $start
    $w.Write([uint32]0x06054b50)
    $w.Write([uint16]0); $w.Write([uint16]0)
    $w.Write([uint16]$written.Count); $w.Write([uint16]$written.Count)
    $w.Write([uint32]$size); $w.Write([uint32]$start); $w.Write([uint16]0)
    $w.Flush()
    $bytes = $stream.ToArray()
    [System.IO.File]::WriteAllBytes($Path, $bytes)
    return $bytes.Length
}

# --- what a cell holds, and how it is written ----------------------------------------------------

# The UTF-16 code units of a string, as upper-case hexadecimal, four digits each.
function Get-Units([string]$Text) {
    $out = New-Object System.Text.StringBuilder
    foreach ($ch in $Text.ToCharArray()) { [void]$out.Append(([int]$ch).ToString('X4')) }
    return $out.ToString()
}

function Get-ColumnLetters([int]$Column) {
    $letters = ''
    while ($Column -gt 0) {
        $digit = ($Column - 1) % 26
        $letters = [string][char](65 + $digit) + $letters
        $Column = [int][math]::Floor(($Column - 1) / 26)
    }
    return $letters
}

function Get-Ref([int]$Row, [int]$Column) { return (Get-ColumnLetters $Column) + $Row }

function Get-XmlEscaped([string]$Text) {
    return $Text.Replace('&', '&amp;').Replace('<', '&lt;').Replace('>', '&gt;').Replace('"', '&quot;')
}

function New-NumberValue([string]$Text) {
    $value = [double]::Parse($Text, [System.Globalization.NumberStyles]::Float, $Invariant)
    $bits = [System.BitConverter]::DoubleToInt64Bits($value).ToString('X16')
    return [ordered]@{ kind = 'number'; text = $Text; bits = $bits }
}

# A number the formula has, computed here: its bits, and its round-trip text for reading.
function New-ComputedNumber([double]$Value) {
    $bits = [System.BitConverter]::DoubleToInt64Bits($Value).ToString('X16')
    return [ordered]@{ kind = 'number'; text = $Value.ToString('R', $Invariant); bits = $bits }
}

function New-TextValue([string]$Text) { return [ordered]@{ kind = 'text'; units = (Get-Units $Text) } }
function New-LogicalValue([bool]$Value) { return [ordered]@{ kind = 'logical'; value = $Value } }
function New-ErrorValue([string]$Spelling) { return [ordered]@{ kind = 'error'; spelling = $Spelling } }

# A sheet being written: its rows of <c> XML and its record.
function New-Sheet([string]$Name, [int]$SheetId, [string]$State, [string]$RelId, [string]$Part) {
    return [ordered]@{
        Name = $Name; SheetId = $SheetId; State = $State; RelId = $RelId; Part = $Part
        Rows = [ordered]@{}; RowAttributes = @{}; Cells = @(); StyledEmpty = @(); Unmaterialized = @()
    }
}

# Rows are keyed by their number as a string: an integer index of an ordered dictionary is a
# position, not a key.
function Add-CellXml($Sheet, [int]$Row, [string]$Xml) {
    $key = [string]$Row
    if (-not $Sheet.Rows.Contains($key)) { $Sheet.Rows[$key] = New-Object System.Text.StringBuilder }
    [void]$Sheet.Rows[$key].Append($Xml)
}

function Get-StyleAttribute([int]$Style) {
    if ($Style -ne 0) { return ' s="' + $Style + '"' }
    return ''
}

function Add-Record($Sheet, [int]$Row, [int]$Column, [int]$Style, $Value, $Formula) {
    $record = [ordered]@{ ref = (Get-Ref $Row $Column); row = $Row; column = $Column; style = $Style; input = $Value }
    if ($null -ne $Formula) {
        foreach ($key in $Formula.Keys) { $record[$key] = $Formula[$key] }
    }
    $Sheet.Cells += , $record
}

# A number constant: $Text is written in <v> as it is.
function Add-Number($Sheet, [int]$Row, [int]$Column, [string]$Text, [int]$Style = 0) {
    Add-CellXml $Sheet $Row ('<c r="' + (Get-Ref $Row $Column) + '"' + (Get-StyleAttribute $Style) + '><v>' + $Text + '</v></c>')
    Add-Record $Sheet $Row $Column $Style (New-NumberValue $Text) $null
}

# A shared string: index $Index of the shared strings, whose text is $Text.
function Add-SharedString($Sheet, [int]$Row, [int]$Column, [int]$Index, [string]$Text, [int]$Style = 0) {
    Add-CellXml $Sheet $Row ('<c r="' + (Get-Ref $Row $Column) + '"' + (Get-StyleAttribute $Style) + ' t="s"><v>' + $Index + '</v></c>')
    Add-Record $Sheet $Row $Column $Style (New-TextValue $Text) $null
}

# An inline string written as $InnerXml (the content of <is>), whose text is $Text.
function Add-InlineString($Sheet, [int]$Row, [int]$Column, [string]$InnerXml, [string]$Text, [int]$Style = 0) {
    Add-CellXml $Sheet $Row ('<c r="' + (Get-Ref $Row $Column) + '"' + (Get-StyleAttribute $Style) + ' t="inlineStr"><is>' + $InnerXml + '</is></c>')
    Add-Record $Sheet $Row $Column $Style (New-TextValue $Text) $null
}

function Add-Logical($Sheet, [int]$Row, [int]$Column, [bool]$Value, [int]$Style = 0) {
    $v = '0'
    if ($Value) { $v = '1' }
    Add-CellXml $Sheet $Row ('<c r="' + (Get-Ref $Row $Column) + '"' + (Get-StyleAttribute $Style) + ' t="b"><v>' + $v + '</v></c>')
    Add-Record $Sheet $Row $Column $Style (New-LogicalValue $Value) $null
}

function Add-Error($Sheet, [int]$Row, [int]$Column, [string]$Spelling, [int]$Style = 0) {
    Add-CellXml $Sheet $Row ('<c r="' + (Get-Ref $Row $Column) + '"' + (Get-StyleAttribute $Style) + ' t="e"><v>' + $Spelling + '</v></c>')
    Add-Record $Sheet $Row $Column $Style (New-ErrorValue $Spelling) $null
}

function Add-StyledEmpty($Sheet, [int]$Row, [int]$Column, [int]$Style) {
    Add-CellXml $Sheet $Row ('<c r="' + (Get-Ref $Row $Column) + '"' + (Get-StyleAttribute $Style) + '/>')
    $Sheet.StyledEmpty += , ([ordered]@{ ref = (Get-Ref $Row $Column); row = $Row; column = $Column; style = $Style })
}

function Add-Unmaterialized($Sheet, [int]$Row, [int]$Column) {
    Add-CellXml $Sheet $Row ('<c r="' + (Get-Ref $Row $Column) + '"/>')
    $Sheet.Unmaterialized += , (Get-Ref $Row $Column)
}

# The t attribute and <v> element of a cached formula result, two empty strings for no cache. (The
# comma binds tighter than +, so each concatenation is parenthesized.)
function Get-CacheXml($Cache) {
    if ($null -eq $Cache) { return @('', '') }
    switch ($Cache.kind) {
        'number' { return @('', ('<v>' + $Cache.text + '</v>')) }
        'text' { return @(' t="str"', ('<v>' + (Get-XmlEscaped $Cache.Source) + '</v>')) }
        'logical' { if ($Cache.value) { return @(' t="b"', '<v>1</v>') } else { return @(' t="b"', '<v>0</v>') } }
        'error' { return @(' t="e"', ('<v>' + $Cache.spelling + '</v>')) }
    }
}

# A text cache: what the formula's cached string is, written in <v> as it is.
function New-TextCache([string]$Text) {
    $cache = New-TextValue $Text
    $cache.Source = $Text
    return $cache
}

# Removes the generator's own fields from a cache before it is recorded.
function Get-RecordedValue($Value) {
    if ($null -eq $Value) { return $null }
    $out = [ordered]@{}
    foreach ($key in $Value.Keys) { if ($key -ne 'Source') { $out[$key] = $Value[$key] } }
    return $out
}

# A formula cell. $Text is the formula's text as the cell holds it; $Written is what is written in
# <f> (empty for a shared formula's member); $FAttributes are <f>'s attributes; $Shared is the
# group (si, master, ref) or $null; $Cache the cached result or $null; $Correct the formula's
# value on this workbook when $Cache was written deliberately wrong, else $null.
function Add-Formula($Sheet, [int]$Row, [int]$Column, [string]$Text, [string]$Written, [string]$FAttributes, $Shared, $Cache, $Correct, [int]$Style = 0) {
    $cacheXml = Get-CacheXml $Cache
    if ($Written -eq '') { $f = '<f' + $FAttributes + '/>' } else { $f = '<f' + $FAttributes + '>' + (Get-XmlEscaped $Written) + '</f>' }
    Add-CellXml $Sheet $Row ('<c r="' + (Get-Ref $Row $Column) + '"' + (Get-StyleAttribute $Style) + $cacheXml[0] + '>' + $f + $cacheXml[1] + '</c>')
    $formula = [ordered]@{ shared = $Shared; cache = (Get-RecordedValue $Cache); correct = (Get-RecordedValue $Correct) }
    Add-Record $Sheet $Row $Column $Style ([ordered]@{ kind = 'formula'; text = $Text }) $formula
}

function Get-SheetXml($Sheet, [string]$Before, [string]$After) {
    $data = New-Object System.Text.StringBuilder
    foreach ($row in $Sheet.Rows.Keys) {
        $attributes = ''
        if ($Sheet.RowAttributes.ContainsKey($row)) { $attributes = $Sheet.RowAttributes[$row] }
        $content = $Sheet.Rows[$row].ToString()
        if ($content -eq '') {
            [void]$data.Append('<row r="' + $row + '"' + $attributes + '/>')
        } else {
            [void]$data.Append('<row r="' + $row + '"' + $attributes + '>' + $content + '</row>')
        }
    }
    return $Decl + '<worksheet xmlns="' + $Main + '" xmlns:r="' + $Rel + '">' + $Before + '<sheetData>' + $data.ToString() + '</sheetData>' + $After + '</worksheet>'
}

function New-Relationships($Items) {
    $text = $Decl + '<Relationships xmlns="' + $PkgRel + '">'
    foreach ($r in $Items) { $text += '<Relationship Id="' + $r[0] + '" Type="' + $r[1] + '" Target="' + $r[2] + '"/>' }
    return $text + '</Relationships>'
}

function New-ContentTypes($Overrides) {
    $text = $Decl + '<Types xmlns="' + $ContentTypesNs + '">' +
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>' +
        '<Default Extension="xml" ContentType="application/xml"/>'
    foreach ($o in $Overrides) { $text += '<Override PartName="/' + $o[0] + '" ContentType="' + $o[1] + '"/>' }
    return $text + '</Types>'
}

$WorkbookType = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml'
$WorksheetType = 'application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml'
$StylesType = 'application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml'
$SharedStringsType = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml'
$PackageRels = New-Relationships @(, @('rId1', "$Rel/officeDocument", 'xl/workbook.xml'))

# The record of one sheet, with its part's identity.
function Get-SheetRecord($Sheet, [byte[]]$Content) {
    return [ordered]@{
        name_units = (Get-Units $Sheet.Name)
        name = $Sheet.Name
        sheet_id = $Sheet.SheetId
        state = $Sheet.State
        relationship = $Sheet.RelId
        part = $Sheet.Part
        part_crc32 = (Get-Crc32 $Content)
        part_size = $Content.Length
        cells = @($Sheet.Cells | Sort-Object { $_.row }, { $_.column })
        styled_empty = @($Sheet.StyledEmpty)
        unmaterialized = @($Sheet.Unmaterialized)
    }
}

# --- cells.xlsx ---------------------------------------------------------------------------------

$smile = [string][char]0xD83D + [char]0xDE00
$gruesse = ' Gr' + [char]0x00FC + [char]0x00DF + 'e, ' + [char]0x6570 + [char]0x636E + ' '
$sharedTexts = @(
    ('Fish & chips <3 ' + $smile),
    $gruesse,
    'boldplain',
    ("line" + [char]13 + [char]10 + "two")
)
$sharedItems = @(
    '<si><t>Fish &amp; chips &lt;3 &#x1F600;</t></si>',
    ('<si><t xml:space="preserve">' + $gruesse + '</t></si>'),
    '<si><r><rPr><b/></rPr><t>bold</t></r><r><t>plain</t></r><rPh sb="0" eb="1"><t>PH</t></rPh><phoneticPr fontId="0"/></si>',
    '<si><t>line_x000D__x000A_two</t></si>'
)
$sharedXml = $Decl + '<sst xmlns="' + $Main + '" count="5" uniqueCount="4">' + ($sharedItems -join '') + '</sst>'

$cells = New-Sheet 'Cells' 3 'visible' 'rId2' 'xl/worksheets/sheet2.xml'
$second = New-Sheet 'Second Sheet' 1 'hidden' 'rId1' 'xl/worksheets/sheet1.xml'

# Row 1: numbers, one styled, and a styled cell without a value.
Add-Number $cells 1 1 '0.1'
Add-Number $cells 1 2 '-2.5' 1
Add-Number $cells 1 3 '1E+308'
Add-Number $cells 1 4 '123456789'
Add-Number $cells 1 5 '6.02214076E+23'
Add-Number $cells 1 6 '0.30000000000000004'
Add-StyledEmpty $cells 1 8 2
$cells.RowAttributes['1'] = ' spans="1:8"'

# Row 2: shared strings and inline strings, and a cell element with nothing in it.
Add-SharedString $cells 2 1 0 $sharedTexts[0]
Add-SharedString $cells 2 2 1 $sharedTexts[1] 2
Add-SharedString $cells 2 3 2 $sharedTexts[2]
Add-InlineString $cells 2 4 '<t><![CDATA[a <b> & c]]></t>' 'a <b> & c'
Add-InlineString $cells 2 5 '<t>a_x000D_b</t>' ('a' + [char]13 + 'b')
Add-InlineString $cells 2 6 '<t>_x005F_x0041_</t>' '_x0041_'
Add-InlineString $cells 2 7 '<r><t>x</t></r><r><rPr><i/></rPr><t>y</t></r>' 'xy'
Add-Unmaterialized $cells 2 8

# Row 3: logicals, a styled cell without a value and a cell element with nothing in it.
Add-Logical $cells 3 1 $true
Add-Logical $cells 3 2 $false 1
Add-StyledEmpty $cells 3 3 2
Add-Unmaterialized $cells 3 4
Add-SharedString $cells 3 5 3 $sharedTexts[3]
$cells.RowAttributes['3'] = ' ht="20" customHeight="1"'

# Row 4: error constants.
$column = 1
foreach ($spelling in @('#NULL!', '#DIV/0!', '#VALUE!', '#REF!', '#NAME?', '#NUM!', '#N/A')) {
    Add-Error $cells 4 $column $spelling
    $column += 1
}

# Row 5: formulas. A5's cache is deliberately wrong: A1*2 is 0.2, and 999 is written.
Add-Formula $cells 5 1 'A1*2' 'A1*2' '' $null (New-NumberValue '999') (New-ComputedNumber (0.1 * 2))
Add-Formula $cells 5 2 'A2&"!"' 'A2&"!"' '' $null (New-TextCache 'stale text') (New-TextValue ($sharedTexts[0] + '!'))
Add-Formula $cells 5 3 'NOT(A3)' 'NOT(A3)' '' $null (New-LogicalValue $true) (New-LogicalValue $false)
Add-Formula $cells 5 4 '1/0' '1/0' '' $null (New-ErrorValue '#N/A') (New-ErrorValue '#DIV/0!')
Add-Formula $cells 5 5 'B1+1' 'B1+1' '' $null $null $null
Add-Formula $cells 5 6 'D1+1' 'D1+1' '' $null (New-NumberValue '123456790') $null 1
Add-Formula $cells 5 7 '_xlfn.CONCAT("A1",A1)' '_xlfn.CONCAT("A1",A1)' '' $null (New-TextCache 'A10.1') $null

# Rows 6 to 10: numbers in A, and three shared formula groups whose caches are written wrong.
for ($r = 6; $r -le 10; $r++) { Add-Number $cells $r 1 ([string]($r - 5)) }
for ($r = 6; $r -le 10; $r++) {
    $shared = [ordered]@{ si = 0; master = 'C6'; ref = 'C6:C10' }
    $text = 'A' + $r + '*2+$A$1'
    $correct = New-ComputedNumber ([double](($r - 5) * 2) + 0.1)
    if ($r -eq 6) {
        Add-Formula $cells $r 3 $text $text ' t="shared" ref="C6:C10" si="0"' $shared (New-NumberValue '0') $correct
    } else {
        Add-Formula $cells $r 3 $text '' ' t="shared" si="0"' $shared (New-NumberValue '0') $correct
    }
}
# A two-by-two group: each member's text is the master's with its relative references moved. B6
# and B7 are empty, C5 is FALSE and D5 is #DIV/0!, so E6 is 1, E7 is 2 and F6 and F7 are #DIV/0!.
# G6 and G7 hold a string literal and a quoted sheet name that look like references: G6 is
# 'Second Sheet'!A1, which is 1.1, and G7 is 'Second Sheet'!A2, which is empty, so 0.
$groupValues = @{ 'E6' = (New-ComputedNumber 1); 'F6' = (New-ErrorValue '#DIV/0!'); 'E7' = (New-ComputedNumber 2); 'F7' = (New-ErrorValue '#DIV/0!') }
for ($r = 6; $r -le 7; $r++) {
    foreach ($c in @(5, 6)) {
        $shared = [ordered]@{ si = 1; master = 'E6'; ref = 'E6:F7' }
        $text = (Get-ColumnLetters ($c - 4)) + $r + '+$B' + $r + '+' + (Get-ColumnLetters ($c - 2)) + '$5'
        $correct = $groupValues[(Get-Ref $r $c)]
        if ($r -eq 6 -and $c -eq 5) {
            Add-Formula $cells $r $c $text $text ' t="shared" ref="E6:F7" si="1"' $shared (New-NumberValue '-1') $correct
        } else {
            Add-Formula $cells $r $c $text '' ' t="shared" si="1"' $shared (New-NumberValue '-1') $correct
        }
    }
    $shared = [ordered]@{ si = 2; master = 'G6'; ref = 'G6:G7' }
    $text = 'IF(A' + $r + '>2,"A6",''Second Sheet''!A' + ($r - 5) + ')'
    if ($r -eq 6) {
        Add-Formula $cells $r 7 $text $text ' t="shared" ref="G6:G7" si="2"' $shared (New-NumberValue '7') (New-ComputedNumber (0.1 + 1))
    } else {
        Add-Formula $cells $r 7 $text '' ' t="shared" si="2"' $shared (New-NumberValue '7') (New-ComputedNumber 0)
    }
}
# A two-by-two group, D8:E9, without caches. Its text holds ranges whose right end is a function
# call (INDEX, OFFSET, _xlfn.XLOOKUP) or a sheet-qualified cell, ranges of rows and of columns with
# one absolute end, mixed references and a string literal that looks like a reference. Each
# member's text is stated here from the member's own row and column, not moved from the master's.
for ($r = 8; $r -le 9; $r++) {
    foreach ($c in @(4, 5)) {
        $shared = [ordered]@{ si = 3; master = 'D8'; ref = 'D8:E9' }
        $a = Get-ColumnLetters ($c - 3)
        $b = Get-ColumnLetters ($c - 2)
        $j = Get-ColumnLetters ($c + 6)
        $text = 'SUM(' + $a + $r + ':INDEX(' + $b + ':' + $b + ',2))' +
            '+SUM(' + $a + $r + ':OFFSET(' + $b + ($r + 1) + ',1,0))' +
            '+COUNT(Cells!' + $a + $r + ':Cells!' + $b + $r + ')' +
            '+SUM(' + $a + $r + ':_xlfn.XLOOKUP(1,' + $a + ':' + $a + ',' + $b + ':' + $b + '))' +
            '+ROWS($20:' + ($r + 12) + ')+COLUMNS($J:' + $j + ')+$A' + $r + '+' + $a + '$1+LEN("A8")'
        if ($r -eq 8 -and $c -eq 4) {
            Add-Formula $cells $r $c $text $text ' t="shared" ref="D8:E9" si="3"' $shared $null $null
        } else {
            Add-Formula $cells $r $c $text '' ' t="shared" si="3"' $shared $null $null
        }
    }
}
# A row without cells.
$cells.Rows['12'] = New-Object System.Text.StringBuilder
$cells.RowAttributes['12'] = ' ht="30" customHeight="1"'

# The second sheet: a formula on the first, its cache deliberately wrong, and an inline string.
Add-Formula $second 1 1 'Cells!A1+1' 'Cells!A1+1' '' $null (New-NumberValue '-1') (New-ComputedNumber (0.1 + 1))
Add-InlineString $second 1 2 '<t>second</t>' 'second'

$cellsXml = Get-SheetXml $cells '<dimension ref="A1:H12"/><sheetViews><sheetView workbookViewId="0"/></sheetViews><sheetFormatPr defaultRowHeight="15"/><cols><col min="1" max="1" width="12" customWidth="1"/></cols>' '<mergeCells count="1"><mergeCell ref="H10:H11"/></mergeCells><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>'
$secondXml = Get-SheetXml $second '<dimension ref="A1:B1"/>' ''
$cellsBytes = $Utf8.GetBytes($cellsXml)
$secondBytes = $Utf8.GetBytes($secondXml)

$workbookXml = $Decl + '<workbook xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><fileVersion appName="xl"/><bookViews><workbookView activeTab="0"/></bookViews>' +
    '<sheets><sheet name="Cells" sheetId="3" r:id="rId2"/><sheet name="Second Sheet" sheetId="1" state="hidden" r:id="rId1"/></sheets>' +
    '<definedNames><definedName name="Rate">Cells!$A$1</definedName></definedNames><calcPr calcId="191029"/></workbook>'
$workbookRels = New-Relationships @(
    @('rId1', "$Rel/worksheet", 'worksheets/sheet1.xml'),
    @('rId2', "$Rel/worksheet", 'worksheets/sheet2.xml'),
    @('rId3', "$Rel/styles", 'styles.xml'),
    @('rId4', "$Rel/sharedStrings", 'sharedStrings.xml')
)
$stylesXml = $Decl + '<styleSheet xmlns="' + $Main + '">' +
    '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>' +
    '<fills count="3"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill>' +
    '<fill><patternFill patternType="solid"><fgColor rgb="FFFFFF00"/><bgColor indexed="64"/></patternFill></fill></fills>' +
    '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>' +
    '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>' +
    '<cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>' +
    '<xf numFmtId="2" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>' +
    '<xf numFmtId="0" fontId="0" fillId="2" borderId="0" xfId="0" applyFill="1"/></cellXfs>' +
    '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>'
$cellsTypes = New-ContentTypes @(
    @('xl/workbook.xml', $WorkbookType),
    @('xl/worksheets/sheet1.xml', $WorksheetType),
    @('xl/worksheets/sheet2.xml', $WorksheetType),
    @('xl/styles.xml', $StylesType),
    @('xl/sharedStrings.xml', $SharedStringsType)
)
$cellsLength = Write-Zip (Join-Path $OutDir 'cells.xlsx') @(
    @('[Content_Types].xml', $Utf8.GetBytes($cellsTypes)),
    @('_rels/.rels', $Utf8.GetBytes($PackageRels)),
    @('xl/workbook.xml', $Utf8.GetBytes($workbookXml)),
    @('xl/_rels/workbook.xml.rels', $Utf8.GetBytes($workbookRels)),
    @('xl/worksheets/sheet1.xml', $secondBytes),
    @('xl/worksheets/sheet2.xml', $cellsBytes),
    @('xl/styles.xml', $Utf8.GetBytes($stylesXml)),
    @('xl/sharedStrings.xml', $Utf8.GetBytes($sharedXml))
)

# --- extent.xlsx and corner.xlsx: one sheet, one cell --------------------------------------------

function Write-OneCellPackage([string]$File, [int]$Row, [int]$Column) {
    $sheet = New-Sheet 'Edge' 1 'visible' 'rId1' 'xl/worksheets/sheet1.xml'
    Add-Number $sheet $Row $Column '42'
    $ref = Get-Ref $Row $Column
    $bytes = $Utf8.GetBytes((Get-SheetXml $sheet ('<dimension ref="' + $ref + '"/>') ''))
    $workbook = $Decl + '<workbook xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><sheets><sheet name="Edge" sheetId="1" r:id="rId1"/></sheets></workbook>'
    $length = Write-Zip (Join-Path $OutDir $File) @(
        @('[Content_Types].xml', $Utf8.GetBytes((New-ContentTypes @(@('xl/workbook.xml', $WorkbookType), @('xl/worksheets/sheet1.xml', $WorksheetType))))),
        @('_rels/.rels', $Utf8.GetBytes($PackageRels)),
        @('xl/workbook.xml', $Utf8.GetBytes($workbook)),
        @('xl/_rels/workbook.xml.rels', $Utf8.GetBytes((New-Relationships @(, @('rId1', "$Rel/worksheet", 'worksheets/sheet1.xml'))))),
        @('xl/worksheets/sheet1.xml', $bytes)
    )
    return [ordered]@{
        file = $File
        length = $length
        workbook_part = 'xl/workbook.xml'
        cell_formats = 0
        date_system = 1900
        defined_names = 0
        sheets = @(, (Get-SheetRecord $sheet $bytes))
    }
}

$extent = Write-OneCellPackage 'extent.xlsx' 1048576 16384
$corner = Write-OneCellPackage 'corner.xlsx' 1 1

$manifest = [ordered]@{
    format = 'cdoc-m1.2-fixture-manifest/1'
    generator = 'make-fixtures.ps1'
    packages = @(
        [ordered]@{
            file = 'cells.xlsx'
            length = $cellsLength
            workbook_part = 'xl/workbook.xml'
            cell_formats = 3
            date_system = 1900
            defined_names = 1
            sheets = @((Get-SheetRecord $cells $cellsBytes), (Get-SheetRecord $second $secondBytes))
        },
        $extent,
        $corner
    )
}
[System.IO.File]::WriteAllText((Join-Path $OutDir 'manifest.json'), (ConvertTo-Json -InputObject $manifest -Depth 12), $Utf8)
