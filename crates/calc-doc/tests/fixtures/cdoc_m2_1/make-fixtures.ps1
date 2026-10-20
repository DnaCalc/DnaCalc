# The generator of the cdoc-m2.1 package fixtures in this directory:
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/notebook.xlsx, manifest-v1.xlsx, manifest-v2.xlsx, manifest-unreadable.xlsx
# and manifest.json. Run with this directory as -OutDir it rewrites the committed fixtures.
#
# All four packages hold the same workbook: a visible, descriptively named backing worksheet
# "Notebook" (sheetId 1) with two prose texts (shared strings) in A1 and A3 and two formulas with
# cached results in B5 and B6, and a sheet "Data" (sheetId 2) with a text and three numbers; a
# workbook-scoped defined name Rate and a sheet-scoped defined name Local; and a custom XML item
# that is not a DNA manifest (customXml/item1.xml, root element in urn:example:unrelated), with its
# item properties and relationships, reached from the workbook part.
#
# - notebook.xlsx holds no DNA view manifest.
# - manifest-v1.xlsx adds a version 1 DNA manifest as customXml/item2.xml (item properties
#   customXml/itemProps2.xml, relationships customXml/_rels/item2.xml.rels, reached from the
#   workbook part): four blocks, three with hidden locator names in the workbook part (one written
#   as Notebook!$A$1, unquoted; two quoted, a cell and a range) and one whose locator name the
#   workbook part does not hold; placements in the notebook, tree, canvas and grid views; and
#   fields version 1 does not name (unknown_fields below): an attribute and an element of the
#   manifest element, an attribute and an element of two blocks, an element in blocks, an element
#   of another namespace in a placement, an attribute of the tree element and of a canvas
#   placement, and an element in views.
# - manifest-v2.xlsx adds a DNA manifest of version 2 in the same parts, whose content version 1
#   does not describe, and hidden locator names for two of its blocks.
# - manifest-unreadable.xlsx adds a version 1 DNA manifest one of whose placements names a block
#   the manifest does not hold, and a hidden locator name for its one block.
#
# manifest.json is the generator's own record of what it wrote, computed here and not by the code
# under test: for each package its sheets (name, sheetId, part), every cell that holds a value or a
# formula (a text as its UTF-16 code units, a number as the text written in <v> and the bits
# .NET's Double.Parse gives it, a formula as its text), its defined names (name, text, hidden,
# localSheetId), the name, CRC-32 and size of each custom XML part, and its DNA manifest as the
# generator stated it: its part, its version, its blocks (ID, kind, locator, and the anchor its
# locator refers to, as a sheet name and an A1 area, or null), its placements per view in order,
# and the exact text of each unknown field it wrote.
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
$DataStore = 'http://schemas.openxmlformats.org/officeDocument/2006/customXml'
$Dna = 'urn:dnacalc:view-manifest'
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

# --- text helpers --------------------------------------------------------------------------------

# The UTF-16 code units of a string, as upper-case hexadecimal, four digits each.
function Get-Units([string]$Text) {
    $out = New-Object System.Text.StringBuilder
    foreach ($ch in $Text.ToCharArray()) { [void]$out.Append(([int]$ch).ToString('X4')) }
    return $out.ToString()
}

function Get-XmlEscaped([string]$Text) {
    return $Text.Replace('&', '&amp;').Replace('<', '&lt;').Replace('>', '&gt;').Replace('"', '&quot;')
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
$SharedStringsType = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml'
$PropsType = 'application/vnd.openxmlformats-officedocument.customXmlProperties+xml'
$PackageRels = New-Relationships @(, @('rId1', "$Rel/officeDocument", 'xl/workbook.xml'))

# --- the workbook every package holds -----------------------------------------------------------

$texts = @(
    'Loan notebook',
    'The principal times the rate, then the total of the inputs.',
    'Principal'
)
$sharedXml = $Decl + '<sst xmlns="' + $Main + '" count="3" uniqueCount="3">' +
    (($texts | ForEach-Object { '<si><t>' + (Get-XmlEscaped $_) + '</t></si>' }) -join '') + '</sst>'

# A number cell: the text written in <v> and the bits .NET's Double.Parse gives it.
function New-NumberCell([string]$Ref, [string]$Text) {
    $value = [double]::Parse($Text, [System.Globalization.NumberStyles]::Float, $Invariant)
    return [ordered]@{ ref = $Ref; kind = 'number'; text = $Text; bits = [System.BitConverter]::DoubleToInt64Bits($value).ToString('X16') }
}

$notebookCells = @(
    [ordered]@{ ref = 'A1'; kind = 'text'; units = (Get-Units $texts[0]) },
    [ordered]@{ ref = 'A3'; kind = 'text'; units = (Get-Units $texts[1]) },
    [ordered]@{ ref = 'B5'; kind = 'formula'; text = 'Data!B1*Rate' },
    [ordered]@{ ref = 'B6'; kind = 'formula'; text = 'SUM(Data!B1:B3)' }
)
$dataCells = @(
    [ordered]@{ ref = 'A1'; kind = 'text'; units = (Get-Units $texts[2]) },
    (New-NumberCell 'B1' '1000'),
    (New-NumberCell 'B2' '5'),
    (New-NumberCell 'B3' '7')
)
$notebookXml = $Decl + '<worksheet xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><dimension ref="A1:B6"/><sheetData>' +
    '<row r="1"><c r="A1" t="s"><v>0</v></c></row>' +
    '<row r="3"><c r="A3" t="s"><v>1</v></c></row>' +
    '<row r="5"><c r="B5"><f>Data!B1*Rate</f><v>5000</v></c></row>' +
    '<row r="6"><c r="B6"><f>SUM(Data!B1:B3)</f><v>1012</v></c></row>' +
    '</sheetData></worksheet>'
$dataXml = $Decl + '<worksheet xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><dimension ref="A1:B3"/><sheetData>' +
    '<row r="1"><c r="A1" t="s"><v>2</v></c><c r="B1"><v>1000</v></c></row>' +
    '<row r="2"><c r="B2"><v>5</v></c></row>' +
    '<row r="3"><c r="B3"><v>7</v></c></row>' +
    '</sheetData></worksheet>'

$baseNames = @(
    [ordered]@{ name = 'Local'; text = 'Notebook!$A$1'; hidden = $false; local_sheet_id = 0 },
    [ordered]@{ name = 'Rate'; text = 'Data!$B$2'; hidden = $false; local_sheet_id = $null }
)

function Get-DefinedNameXml($Name) {
    $attributes = ' name="' + $Name.name + '"'
    if ($null -ne $Name.local_sheet_id) { $attributes += ' localSheetId="' + $Name.local_sheet_id + '"' }
    if ($Name.hidden) { $attributes += ' hidden="1"' }
    return '<definedName' + $attributes + '>' + (Get-XmlEscaped $Name.text) + '</definedName>'
}

function Get-WorkbookXml($Names) {
    return $Decl + '<workbook xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><fileVersion appName="xl"/>' +
        '<bookViews><workbookView activeTab="0"/></bookViews>' +
        '<sheets><sheet name="Notebook" sheetId="1" r:id="rId1"/><sheet name="Data" sheetId="2" r:id="rId2"/></sheets>' +
        '<definedNames>' + (($Names | ForEach-Object { Get-DefinedNameXml $_ }) -join '') + '</definedNames>' +
        '<calcPr calcId="191029"/></workbook>'
}

function Get-PropsXml([string]$ItemId, [string]$Schema) {
    return '<?xml version="1.0" encoding="UTF-8" standalone="no"?>' + "`n" +
        '<ds:datastoreItem ds:itemID="' + $ItemId + '" xmlns:ds="' + $DataStore + '"><ds:schemaRefs><ds:schemaRef ds:uri="' + $Schema + '"/></ds:schemaRefs></ds:datastoreItem>'
}

function Get-ItemRelsXml([int]$N) {
    return New-Relationships @(, @('rId1', "$Rel/customXmlProps", ('itemProps' + $N + '.xml')))
}

$unrelatedXml = $Decl + '<properties xmlns="urn:example:unrelated"><owner>generator</owner></properties>'

# Writes one package of the workbook, with $Names as its defined names and, when $ManifestXml is
# not $null, a DNA manifest as customXml/item2.xml. Returns its record.
function Write-Package([string]$File, $Names, $ManifestXml, $ManifestRecord) {
    $parts = @(
        @('xl/workbook.xml', $Utf8.GetBytes((Get-WorkbookXml $Names))),
        @('xl/worksheets/sheet1.xml', $Utf8.GetBytes($notebookXml)),
        @('xl/worksheets/sheet2.xml', $Utf8.GetBytes($dataXml)),
        @('xl/sharedStrings.xml', $Utf8.GetBytes($sharedXml)),
        @('customXml/item1.xml', $Utf8.GetBytes($unrelatedXml)),
        @('customXml/itemProps1.xml', $Utf8.GetBytes((Get-PropsXml '{6A0B9C3D-1E2F-4A5B-8C7D-0E1F2A3B4C5D}' 'urn:example:unrelated'))),
        @('customXml/_rels/item1.xml.rels', $Utf8.GetBytes((Get-ItemRelsXml 1)))
    )
    $relationships = @(
        @('rId1', "$Rel/worksheet", 'worksheets/sheet1.xml'),
        @('rId2', "$Rel/worksheet", 'worksheets/sheet2.xml'),
        @('rId3', "$Rel/sharedStrings", 'sharedStrings.xml'),
        @('rId4', "$Rel/customXml", '../customXml/item1.xml')
    )
    $overrides = @(
        @('xl/workbook.xml', $WorkbookType),
        @('xl/worksheets/sheet1.xml', $WorksheetType),
        @('xl/worksheets/sheet2.xml', $WorksheetType),
        @('xl/sharedStrings.xml', $SharedStringsType),
        @('customXml/itemProps1.xml', $PropsType)
    )
    if ($null -ne $ManifestXml) {
        $parts += , @('customXml/item2.xml', $Utf8.GetBytes($ManifestXml))
        $parts += , @('customXml/itemProps2.xml', $Utf8.GetBytes((Get-PropsXml '{0D9E3A41-7C25-4B86-9F10-2E4D6B8A1C37}' $Dna)))
        $parts += , @('customXml/_rels/item2.xml.rels', $Utf8.GetBytes((Get-ItemRelsXml 2)))
        $relationships += , @('rId5', "$Rel/customXml", '../customXml/item2.xml')
        $overrides += , @('customXml/itemProps2.xml', $PropsType)
    }
    $entries = @(
        @('[Content_Types].xml', $Utf8.GetBytes((New-ContentTypes $overrides))),
        @('_rels/.rels', $Utf8.GetBytes($PackageRels)),
        @('xl/_rels/workbook.xml.rels', $Utf8.GetBytes((New-Relationships $relationships)))
    ) + $parts
    $length = Write-Zip (Join-Path $OutDir $File) $entries
    $custom = @()
    foreach ($e in $entries) {
        if ($e[0].StartsWith('customXml/')) {
            $custom += , ([ordered]@{ part = $e[0]; crc32 = (Get-Crc32 $e[1]); size = $e[1].Length })
        }
    }
    return [ordered]@{
        file = $File
        length = $length
        sheets = @(
            [ordered]@{ name = 'Notebook'; sheet_id = 1; part = 'xl/worksheets/sheet1.xml'; cells = $notebookCells },
            [ordered]@{ name = 'Data'; sheet_id = 2; part = 'xl/worksheets/sheet2.xml'; cells = $dataCells }
        )
        defined_names = @($Names)
        custom_xml = $custom
        dna_manifest = $ManifestRecord
    }
}

function Get-Locator([string]$BlockId) { return '_dna.block.' + $BlockId.Replace('-', '') }

# --- notebook.xlsx -------------------------------------------------------------------------------

$notebook = Write-Package 'notebook.xlsx' $baseNames $null $null

# --- manifest-v1.xlsx ----------------------------------------------------------------------------

$m = '0f3e7a21-5c6b-4d8e-a1f2-3b4c5d6e7f0'
$p = '7d2c1b0a-9e8f-4a6b-b5c4-d3e2f1a0b9'
$m1 = $m + '1'; $m2 = $m + '2'; $m3 = $m + '3'; $m4 = $m + '4'
$v1Names = $baseNames + @(
    [ordered]@{ name = (Get-Locator $m1); text = 'Notebook!$A$1'; hidden = $true; local_sheet_id = $null },
    [ordered]@{ name = (Get-Locator $m2); text = "'Notebook'!`$B`$5"; hidden = $true; local_sheet_id = $null },
    [ordered]@{ name = (Get-Locator $m3); text = "'Notebook'!`$B`$5:`$B`$6"; hidden = $true; local_sheet_id = $null }
)
# Each unknown field, exactly as written into the manifest.
$unknownRoot = '<manifest xmlns="' + $Dna + '" version="1" generator="make-fixtures.ps1">'
$unknownBlock1 = '<block id="' + $m1 + '" kind="prose" locator="' + (Get-Locator $m1) + '" style="lead"/>'
$unknownBlock2 = '<block id="' + $m2 + '" kind="calculation" locator="' + (Get-Locator $m2) + '"><markdown>**Payment** at the rate</markdown></block>'
$unknownGroup = '<group name="g1"/>'
$unknownAnnotations = '<annotations><note>kept</note></annotations>'
$unknownPlacement = '<placement id="' + $p + '12" block="' + $m2 + '"><x:extra xmlns:x="urn:example:ext" note="1"/></placement>'
$unknownTree = '<tree expanded="all">'
$unknownZoom = 'zoom="2"'
$unknownTimeline = '<timeline><placement id="' + $p + '51" block="' + $m1 + '" at="2026-10-20"/></timeline>'
$v1Manifest = $Decl + $unknownRoot + "`n" +
    '<blocks>' + "`n" +
    $unknownBlock1 + "`n" +
    $unknownBlock2 + "`n" +
    '<block id="' + $m3 + '" kind="calculation" locator="' + (Get-Locator $m3) + '"/>' + "`n" +
    '<block id="' + $m4 + '" kind="prose" locator="' + (Get-Locator $m4) + '"/>' + "`n" +
    $unknownGroup + "`n" +
    '</blocks>' + "`n" +
    $unknownAnnotations + "`n" +
    '<views>' + "`n" +
    '<notebook><placement id="' + $p + '11" block="' + $m1 + '"/>' + $unknownPlacement + '<placement id="' + $p + '13" block="' + $m3 + '"/></notebook>' + "`n" +
    $unknownTree + '<placement id="' + $p + '21" block="' + $m1 + '"/><placement id="' + $p + '22" block="' + $m2 + '" parent="' + $p + '21"/><placement id="' + $p + '23" block="' + $m3 + '" parent="' + $p + '21"/></tree>' + "`n" +
    '<canvas><placement id="' + $p + '31" block="' + $m2 + '" x="10" y="20" width="200" height="80" ' + $unknownZoom + '/><placement id="' + $p + '32" block="' + $m3 + '" x="-35.5" y="140" width="240" height="120"/></canvas>' + "`n" +
    '<grid><placement id="' + $p + '41" block="' + $m2 + '"/><placement id="' + $p + '42" block="' + $m4 + '"/></grid>' + "`n" +
    $unknownTimeline + "`n" +
    '</views>' + "`n" +
    '</manifest>'
$v1Record = [ordered]@{
    part = 'customXml/item2.xml'
    version = '1'
    blocks = @(
        [ordered]@{ id = $m1; kind = 'prose'; locator = (Get-Locator $m1); anchor = [ordered]@{ sheet = 'Notebook'; ref = 'A1' } },
        [ordered]@{ id = $m2; kind = 'calculation'; locator = (Get-Locator $m2); anchor = [ordered]@{ sheet = 'Notebook'; ref = 'B5' } },
        [ordered]@{ id = $m3; kind = 'calculation'; locator = (Get-Locator $m3); anchor = [ordered]@{ sheet = 'Notebook'; ref = 'B5:B6' } },
        [ordered]@{ id = $m4; kind = 'prose'; locator = (Get-Locator $m4); anchor = $null }
    )
    notebook = @(
        [ordered]@{ id = ($p + '11'); block = $m1 },
        [ordered]@{ id = ($p + '12'); block = $m2 },
        [ordered]@{ id = ($p + '13'); block = $m3 }
    )
    tree = @(
        [ordered]@{ id = ($p + '21'); block = $m1; parent = $null },
        [ordered]@{ id = ($p + '22'); block = $m2; parent = ($p + '21') },
        [ordered]@{ id = ($p + '23'); block = $m3; parent = ($p + '21') }
    )
    canvas = @(
        [ordered]@{ id = ($p + '31'); block = $m2; x = '10'; y = '20'; width = '200'; height = '80' },
        [ordered]@{ id = ($p + '32'); block = $m3; x = '-35.5'; y = '140'; width = '240'; height = '120' }
    )
    grid = @(
        [ordered]@{ id = ($p + '41'); block = $m2 },
        [ordered]@{ id = ($p + '42'); block = $m4 }
    )
    unknown_fields = @($unknownRoot, $unknownBlock1, $unknownBlock2, $unknownGroup, $unknownAnnotations, $unknownPlacement, $unknownTree, $unknownZoom, $unknownTimeline)
}
$v1 = Write-Package 'manifest-v1.xlsx' $v1Names $v1Manifest $v1Record

# --- manifest-v2.xlsx ----------------------------------------------------------------------------

$w = '3a9b1c7d-2e4f-4a60-9b8c-7d6e5f4a3b2'
$v2Names = $baseNames + @(
    [ordered]@{ name = (Get-Locator ($w + '1')); text = "'Notebook'!`$A`$1"; hidden = $true; local_sheet_id = $null },
    [ordered]@{ name = (Get-Locator ($w + '2')); text = "'Notebook'!`$B`$5"; hidden = $true; local_sheet_id = $null }
)
$v2Manifest = $Decl + '<manifest xmlns="' + $Dna + '" version="2"><layout><block ref="' + $w + '1" role="heading"/><block ref="' + $w + '2" role="result"/></layout><stack><item block="' + $w + '1"/></stack></manifest>'
$v2Record = [ordered]@{ part = 'customXml/item2.xml'; version = '2'; blocks = @(); notebook = @(); tree = @(); canvas = @(); grid = @(); unknown_fields = @() }
$v2 = Write-Package 'manifest-v2.xlsx' $v2Names $v2Manifest $v2Record

# --- manifest-unreadable.xlsx --------------------------------------------------------------------

$u = '6c5d4e3f-2a1b-4c0d-8e9f-a0b1c2d3e4f'
$unreadableNames = $baseNames + @(
    , [ordered]@{ name = (Get-Locator ($u + '1')); text = "'Notebook'!`$A`$1"; hidden = $true; local_sheet_id = $null }
)
$unreadableManifest = $Decl + '<manifest xmlns="' + $Dna + '" version="1"><blocks><block id="' + $u + '1" kind="prose" locator="' + (Get-Locator ($u + '1')) + '"/></blocks>' +
    '<views><notebook><placement id="' + $u + '2" block="' + $u + '9"/></notebook></views></manifest>'
$unreadableRecord = [ordered]@{ part = 'customXml/item2.xml'; version = '1'; blocks = @(); notebook = @(); tree = @(); canvas = @(); grid = @(); unknown_fields = @() }
$unreadable = Write-Package 'manifest-unreadable.xlsx' $unreadableNames $unreadableManifest $unreadableRecord

$manifest = [ordered]@{
    format = 'cdoc-m2.1-fixture-manifest/1'
    generator = 'make-fixtures.ps1'
    packages = @($notebook, $v1, $v2, $unreadable)
}
[System.IO.File]::WriteAllText((Join-Path $OutDir 'manifest.json'), (ConvertTo-Json -InputObject $manifest -Depth 12), $Utf8)
