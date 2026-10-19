# The generator of the cdoc-m1.1 package fixtures in this directory:
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-fixtures.ps1 -OutDir <directory>
#
# writes <directory>/package.xlsx, <directory>/lying-size.xlsx and <directory>/manifest.json. Run
# with this directory as -OutDir it rewrites the committed fixtures.
#
# The ZIP container is written by the writer below, field by field. Part payloads are deflated by
# .NET's DeflateStream (System.IO.Compression, CompressionLevel Optimal), so the compressed bytes
# depend on the .NET Framework that runs the script; the committed fixtures and their manifest
# were written together by one run under Windows PowerShell 5.1. manifest.json records what the
# generator wrote, computed here and not by the code under test: each entry's fields and offsets,
# the SHA-256 (.NET's) of its compressed payload and of its content, the content type the
# generator assigned, each relationship with the part name its target denotes, and for each XML
# part the counts and text .NET's XmlDocument reads from it.
#
# package.xlsx is a SpreadsheetML package with two worksheets, shared strings, styles, core
# properties and a printer-settings binary. Its archive exercises what a preserving writer must
# keep: deflated and stored entries, data descriptors with and without their signature, local and
# central extra fields that differ, an entry comment, an archive comment, a directory entry,
# 37 unreferenced bytes between two entries, and a central directory whose order differs from the
# order the entries are stored in ([Content_Types].xml is stored last and listed first).
#
# lying-size.xlsx holds xl/media/zeros.bin, whose deflate stream expands to 64 MiB of zeros while
# its entry declares 4096 bytes and the CRC-32 of 4096 zeros, so a reader that stopped silently at
# the declared size would find the CRC it expects.
#
# The script is ASCII and runs under Windows PowerShell 5.1.

param(
    [Parameter(Mandatory = $true)][string]$OutDir
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
if (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path (Get-Location).Path $OutDir }
if (-not (Test-Path -LiteralPath $OutDir)) { [void](New-Item -ItemType Directory -Path $OutDir) }

$Utf8 = New-Object System.Text.UTF8Encoding($false)
$Main = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'
$Rel = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
$PkgRel = 'http://schemas.openxmlformats.org/package/2006/relationships'
$ContentTypesNs = 'http://schemas.openxmlformats.org/package/2006/content-types'
$Decl = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' + "`n"

# --- CRC-32, SHA-256, deflate --------------------------------------------------------------------

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

$Sha = [System.Security.Cryptography.SHA256]::Create()
function Get-Sha256([byte[]]$Bytes) {
    return ([System.BitConverter]::ToString($Sha.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant()
}

# Raw DEFLATE (no zlib header) of the chunks, each a byte array, written in order.
function Get-Deflated($Chunks) {
    $memory = New-Object System.IO.MemoryStream
    $deflate = New-Object System.IO.Compression.DeflateStream($memory, [System.IO.Compression.CompressionLevel]::Optimal, $true)
    foreach ($chunk in $Chunks) { $deflate.Write($chunk, 0, $chunk.Length) }
    $deflate.Dispose()
    return , $memory.ToArray()
}

# --- the ZIP writer ------------------------------------------------------------------------------

# An entry to write. $Content is the content (byte array); method 0 stores it, 8 deflates it.
# Descriptor is 0 (sizes in the local header), 12 (a data descriptor without its signature) or 16
# (with it). DeclaredSize and DeclaredCrc replace the true values in every record when set;
# Payload replaces the compressed bytes when set.
function New-Entry([string]$Name, [byte[]]$Content, [int]$Method) {
    return [ordered]@{
        Name = $Name; Content = $Content; Method = $Method; Descriptor = 0
        LocalExtra = [byte[]]@(); CentralExtra = [byte[]]@(); Comment = ''; Gap = 0
        DeclaredSize = $null; DeclaredCrc = $null; Payload = $null
    }
}

# Writes the entries in the order given and the central directory in $CentralOrder (names).
# Returns, per entry name, what was written.
function Write-Zip([string]$Path, $Entries, [string[]]$CentralOrder, [string]$ArchiveComment) {
    $stream = New-Object System.IO.MemoryStream
    $w = New-Object System.IO.BinaryWriter($stream)
    $written = [ordered]@{}
    foreach ($e in $Entries) {
        if ($e.Gap -gt 0) {
            $gap = $Utf8.GetBytes(('unreferenced bytes between entries ' * 4).Substring(0, $e.Gap))
            $w.Write($gap)
        }
        $name = $Utf8.GetBytes($e.Name)
        $content = $e.Content
        if ($null -ne $e.Payload) { $payload = $e.Payload }
        elseif ($e.Method -eq 8) { $payload = Get-Deflated @(, $content) }
        else { $payload = $content }
        if ($null -ne $e.DeclaredCrc) { $crc = [uint32]$e.DeclaredCrc } else { $crc = Get-Crc32 $content }
        if ($null -ne $e.DeclaredSize) { $size = [uint32]$e.DeclaredSize } else { $size = [uint32]$content.Length }
        $flags = 0
        if ($e.Descriptor -gt 0) { $flags = 8 }
        $offset = [uint32]$stream.Position
        $w.Write([uint32]0x04034b50)
        $w.Write([uint16]20); $w.Write([uint16]$flags); $w.Write([uint16]$e.Method)
        $w.Write([uint16]0); $w.Write([uint16]0x0021)
        if ($e.Descriptor -gt 0) {
            $w.Write([uint32]0); $w.Write([uint32]0); $w.Write([uint32]0)
        } else {
            $w.Write([uint32]$crc); $w.Write([uint32]$payload.Length); $w.Write([uint32]$size)
        }
        $w.Write([uint16]$name.Length); $w.Write([uint16]$e.LocalExtra.Length)
        $w.Write($name); $w.Write([byte[]]$e.LocalExtra)
        $w.Write([byte[]]$payload)
        if ($e.Descriptor -eq 16) { $w.Write([uint32]0x08074b50) }
        if ($e.Descriptor -gt 0) { $w.Write([uint32]$crc); $w.Write([uint32]$payload.Length); $w.Write([uint32]$size) }
        $written[$e.Name] = [ordered]@{
            Entry = $e; NameBytes = $name; Flags = $flags; Crc = $crc; Size = $size
            Payload = $payload; Offset = $offset
        }
    }
    $start = [uint32]$stream.Position
    foreach ($entryName in $CentralOrder) {
        $x = $written[$entryName]
        $e = $x.Entry
        $comment = $Utf8.GetBytes($e.Comment)
        $w.Write([uint32]0x02014b50)
        $w.Write([uint16]20); $w.Write([uint16]20); $w.Write([uint16]$x.Flags); $w.Write([uint16]$e.Method)
        $w.Write([uint16]0); $w.Write([uint16]0x0021)
        $w.Write([uint32]$x.Crc); $w.Write([uint32]$x.Payload.Length); $w.Write([uint32]$x.Size)
        $w.Write([uint16]$x.NameBytes.Length); $w.Write([uint16]$e.CentralExtra.Length); $w.Write([uint16]$comment.Length)
        $w.Write([uint16]0); $w.Write([uint16]0); $w.Write([uint32]0)
        $w.Write([uint32]$x.Offset)
        $w.Write([byte[]]$x.NameBytes); $w.Write([byte[]]$e.CentralExtra); $w.Write([byte[]]$comment)
    }
    $size = [uint32]$stream.Position - $start
    $comment = $Utf8.GetBytes($ArchiveComment)
    $w.Write([uint32]0x06054b50)
    $w.Write([uint16]0); $w.Write([uint16]0)
    $w.Write([uint16]$CentralOrder.Count); $w.Write([uint16]$CentralOrder.Count)
    $w.Write([uint32]$size); $w.Write([uint32]$start); $w.Write([uint16]$comment.Length)
    $w.Write([byte[]]$comment)
    $w.Flush()
    $bytes = $stream.ToArray()
    [System.IO.File]::WriteAllBytes($Path, $bytes)
    return [ordered]@{ Written = $written; Length = $bytes.Length }
}

# --- package content ----------------------------------------------------------------------------

function New-Relationships($Items) {
    $text = $Decl + '<Relationships xmlns="' + $PkgRel + '">'
    foreach ($r in $Items) {
        $mode = ''
        if ($r.Mode -eq 'External') { $mode = ' TargetMode="External"' }
        $text += '<Relationship Id="' + $r.Id + '" Type="' + $r.Type + '" Target="' + $r.Written + '"' + $mode + '/>'
    }
    return $text + '</Relationships>'
}

# A relationship: Target is the target as it reads after XML unescaping, Written as it is written
# in the part, Resolved the part name it denotes (or $null).
function New-Rel([string]$Id, [string]$Type, [string]$Target, [string]$Written, $Resolved, [string]$Mode = 'Internal') {
    return [ordered]@{ Id = $Id; Type = $Type; Target = $Target; Written = $Written; Resolved = $Resolved; Mode = $Mode }
}

$Defaults = @(
    @('rels', 'application/vnd.openxmlformats-package.relationships+xml'),
    @('xml', 'application/xml'),
    @('bin', 'application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings')
)
$Overrides = @(
    @('/xl/workbook.xml', 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml'),
    @('/xl/worksheets/sheet1.xml', 'application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml'),
    @('/xl/worksheets/sheet2.xml', 'application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml'),
    @('/xl/styles.xml', 'application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml'),
    @('/xl/sharedStrings.xml', 'application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml'),
    @('/docProps/core.xml', 'application/vnd.openxmlformats-package.core-properties+xml')
)

function New-ContentTypes($DefaultList, $OverrideList) {
    $text = $Decl + '<Types xmlns="' + $ContentTypesNs + '">'
    foreach ($d in $DefaultList) { $text += '<Default Extension="' + $d[0] + '" ContentType="' + $d[1] + '"/>' }
    foreach ($o in $OverrideList) { $text += '<Override PartName="' + $o[0] + '" ContentType="' + $o[1] + '"/>' }
    return $text + '</Types>'
}

# The content type the generator assigned a part: its override, else its extension's default.
function Get-AssignedType([string]$Name, $DefaultList, $OverrideList) {
    foreach ($o in $OverrideList) { if ($o[0] -eq ('/' + $Name)) { return $o[1] } }
    $extension = $Name.Substring($Name.LastIndexOf('.') + 1)
    foreach ($d in $DefaultList) { if ($d[0] -eq $extension) { return $d[1] } }
    return $null
}

$PackageRels = @(
    (New-Rel 'rId1' "$Rel/officeDocument" 'xl/workbook.xml' 'xl/workbook.xml' 'xl/workbook.xml'),
    (New-Rel 'rId2' 'http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties' 'docProps/core.xml' 'docProps/core.xml' 'docProps/core.xml')
)
$WorkbookRels = @(
    (New-Rel 'rId1' "$Rel/worksheet" 'worksheets/sheet1.xml' 'worksheets/sheet1.xml' 'xl/worksheets/sheet1.xml'),
    (New-Rel 'rId2' "$Rel/worksheet" '/xl/worksheets/sheet2.xml' '/xl/worksheets/sheet2.xml' 'xl/worksheets/sheet2.xml'),
    (New-Rel 'rId3' "$Rel/styles" 'styles.xml' 'styles.xml' 'xl/styles.xml'),
    (New-Rel 'rId4' "$Rel/sharedStrings" './sharedStrings.xml' './sharedStrings.xml' 'xl/sharedStrings.xml')
)
$SheetRels = @(
    (New-Rel 'rId1' "$Rel/hyperlink" 'https://example.com/report?a=1&b=2' 'https://example.com/report?a=1&amp;b=2' $null 'External'),
    (New-Rel 'rId2' "$Rel/printerSettings" '../printerSettings/printerSettings1.bin' '../printerSettings/printerSettings1.bin' 'xl/printerSettings/printerSettings1.bin')
)

$workbook = $Decl + '<workbook xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><bookViews><workbookView activeTab="0"/></bookViews>' +
    '<sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Notes" sheetId="2" state="hidden" r:id="rId2"/></sheets>' +
    '<calcPr calcId="191029" fullCalcOnLoad="1"/></workbook>'

$rows = New-Object System.Text.StringBuilder
for ($r = 1; $r -le 2000; $r++) {
    [void]$rows.Append('<row r="' + $r + '"><c r="A' + $r + '"><v>' + ($r * 3) + '</v></c><c r="B' + $r + '" t="s"><v>' + ($r % 3) + '</v></c>' +
        '<c r="C' + $r + '"><f>A' + $r + '*2+1</f><v>' + ($r * 6 + 1) + '</v></c><c r="D' + $r + '"><v>' + ($r / 8) + '</v></c></row>')
}
$sheet1 = $Decl + '<worksheet xmlns="' + $Main + '" xmlns:r="' + $Rel + '"><dimension ref="A1:D2000"/><sheetData>' + $rows.ToString() + '</sheetData>' +
    '<hyperlinks><hyperlink ref="A1" r:id="rId1"/></hyperlinks>' +
    '<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="9" orientation="portrait" r:id="rId2"/></worksheet>'
$sheet2 = $Decl + '<worksheet xmlns="' + $Main + '"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t><![CDATA[a <b> & c]]></t></is></c></row></sheetData></worksheet>'

$gruesse = 'Gr' + [char]0x00FC + [char]0x00DF + 'e, ' + [char]0x6570 + [char]0x636E
$shared = $Decl + '<sst xmlns="' + $Main + '" count="2000" uniqueCount="3">' +
    '<si><t>Fish &amp; chips &lt;3 &#x1F600;</t></si>' +
    '<si><t xml:space="preserve"> ' + $gruesse + ' </t></si>' +
    '<si><r><rPr><b/></rPr><t>bold</t></r><r><t>plain</t></r></si></sst>'

$styles = $Decl + '<styleSheet xmlns="' + $Main + '">' +
    '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>' +
    '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>' +
    '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>' +
    '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>' +
    '<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>' +
    '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>'

$core = $Decl + '<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" ' +
    'xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">' +
    '<dc:creator>calc-doc fixture generator</dc:creator><dcterms:created xsi:type="dcterms:W3CDTF">2026-10-19T00:00:00Z</dcterms:created></cp:coreProperties>'

# 600 bytes of a fixed linear congruential sequence: binary content that is not XML.
$printer = New-Object 'byte[]' 600
[int64]$state = 12345
for ($i = 0; $i -lt 600; $i++) { $state = ($state * 1103515245 + 12345) % 2147483648; $printer[$i] = [byte](($state -shr 16) -band 255) }

# Extended-timestamp extra fields (header ID 0x5455): the local one carries a modification time,
# the central one only its flags.
$localExtra = [byte[]]@(0x55, 0x54, 5, 0, 1, 0x80, 0x2F, 0x13, 0x69)
$centralExtra = [byte[]]@(0x55, 0x54, 1, 0, 1)

$e = @{}
$e.rels = New-Entry '_rels/.rels' $Utf8.GetBytes((New-Relationships $PackageRels)) 8
$e.core = New-Entry 'docProps/core.xml' $Utf8.GetBytes($core) 8
$e.workbook = New-Entry 'xl/workbook.xml' $Utf8.GetBytes($workbook) 8
$e.workbook.Descriptor = 16
$e.workbookRels = New-Entry 'xl/_rels/workbook.xml.rels' $Utf8.GetBytes((New-Relationships $WorkbookRels)) 8
$e.workbookRels.Descriptor = 12
$e.sheet1 = New-Entry 'xl/worksheets/sheet1.xml' $Utf8.GetBytes($sheet1) 8
$e.sheetRels = New-Entry 'xl/worksheets/_rels/sheet1.xml.rels' $Utf8.GetBytes((New-Relationships $SheetRels)) 0
$e.sheet2 = New-Entry 'xl/worksheets/sheet2.xml' $Utf8.GetBytes($sheet2) 8
$e.sheet2.Gap = 37
$e.sheet2.Comment = 'kept with its entry'
$e.styles = New-Entry 'xl/styles.xml' $Utf8.GetBytes($styles) 8
$e.styles.LocalExtra = $localExtra
$e.styles.CentralExtra = $centralExtra
$e.shared = New-Entry 'xl/sharedStrings.xml' $Utf8.GetBytes($shared) 8
$e.folder = New-Entry 'xl/printerSettings/' ([byte[]]@()) 0
$e.printer = New-Entry 'xl/printerSettings/printerSettings1.bin' $printer 0
$e.types = New-Entry '[Content_Types].xml' $Utf8.GetBytes((New-ContentTypes $Defaults $Overrides)) 8

$stored = @($e.rels, $e.core, $e.workbook, $e.workbookRels, $e.sheet1, $e.sheetRels, $e.sheet2, $e.styles, $e.shared, $e.folder, $e.printer, $e.types)
$central = @('[Content_Types].xml') + @($stored | Where-Object { $_.Name -ne '[Content_Types].xml' } | ForEach-Object { $_.Name })
$archiveComment = 'cdoc-m1.1 package fixture'
$result = Write-Zip (Join-Path $OutDir 'package.xlsx') $stored $central $archiveComment

# What .NET's XmlDocument reads from an XML part: its document element, its element and
# attribute counts (namespace declarations excluded) and the SHA-256 of the UTF-8 of its text.
function Get-XmlFacts([byte[]]$Content) {
    $document = New-Object System.Xml.XmlDocument
    $document.PreserveWhitespace = $true
    $document.XmlResolver = $null
    $document.LoadXml($Utf8.GetString($Content))
    $elements = $document.SelectNodes('//*')
    $attributes = 0
    foreach ($element in $elements) {
        foreach ($attribute in $element.Attributes) {
            if ($attribute.Name -ne 'xmlns' -and $attribute.Prefix -ne 'xmlns') { $attributes += 1 }
        }
    }
    return [ordered]@{
        namespace = $document.DocumentElement.NamespaceURI
        root = $document.DocumentElement.LocalName
        elements = $elements.Count
        attributes = $attributes
        text_sha256 = (Get-Sha256 $Utf8.GetBytes($document.DocumentElement.InnerText))
    }
}

$manifestEntries = @()
foreach ($entryName in $central) {
    $x = $result.Written[$entryName]
    $entry = $x.Entry
    $isPart = ($entryName -ne '[Content_Types].xml') -and (-not $entryName.EndsWith('/'))
    $type = $null
    if ($isPart) { $type = Get-AssignedType $entryName $Defaults $Overrides }
    $xml = $null
    if ($entryName.EndsWith('.xml') -or $entryName.EndsWith('.rels')) { $xml = Get-XmlFacts $entry.Content }
    $manifestEntries += , ([ordered]@{
        name = $entryName
        part = $isPart
        method = $entry.Method
        flags = $x.Flags
        crc32 = $x.Crc
        compressed_size = $x.Payload.Length
        size = $x.Size
        local_offset = $x.Offset
        descriptor = $entry.Descriptor
        local_extra = $entry.LocalExtra.Length
        central_extra = $entry.CentralExtra.Length
        comment = $entry.Comment
        payload_sha256 = (Get-Sha256 $x.Payload)
        content_sha256 = (Get-Sha256 $entry.Content)
        content_type = $type
        xml = $xml
    })
}

function Get-RelFacts([string]$Part, $Source, $Items) {
    $list = @()
    foreach ($r in $Items) {
        $list += , ([ordered]@{ id = $r.Id; type = $r.Type; target = $r.Target; mode = $r.Mode; resolved = $r.Resolved })
    }
    return [ordered]@{ part = $Part; source = $Source; items = $list }
}

# --- lying-size.xlsx ---------------------------------------------------------------------------

$zeros = New-Object 'byte[]' 1048576
$chunks = @()
for ($i = 0; $i -lt 64; $i++) { $chunks += , $zeros }
$bomb = New-Entry 'xl/media/zeros.bin' (New-Object 'byte[]' 4096) 8
$bomb.Payload = Get-Deflated $chunks
$bomb.DeclaredSize = 4096
$bombDefaults = @(@('rels', 'application/vnd.openxmlformats-package.relationships+xml'), @('xml', 'application/xml'), @('bin', 'application/octet-stream'))
$bombOverrides = @(, @('/xl/workbook.xml', 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml'))
$bombWorkbook = $Decl + '<workbook xmlns="' + $Main + '"><sheets/></workbook>'
$bombStored = @(
    (New-Entry '[Content_Types].xml' $Utf8.GetBytes((New-ContentTypes $bombDefaults $bombOverrides)) 8),
    (New-Entry '_rels/.rels' $Utf8.GetBytes((New-Relationships @(, $PackageRels[0]))) 8),
    (New-Entry 'xl/workbook.xml' $Utf8.GetBytes($bombWorkbook) 8),
    $bomb
)
$bombResult = Write-Zip (Join-Path $OutDir 'lying-size.xlsx') $bombStored @($bombStored | ForEach-Object { $_.Name }) ''

$manifest = [ordered]@{
    format = 'cdoc-m1.1-fixture-manifest/1'
    generator = 'make-fixtures.ps1'
    package = [ordered]@{
        file = 'package.xlsx'
        length = $result.Length
        archive_comment = $archiveComment
        unreferenced_bytes = 37
        entries = $manifestEntries
        relationships = @(
            (Get-RelFacts '_rels/.rels' $null $PackageRels),
            (Get-RelFacts 'xl/_rels/workbook.xml.rels' 'xl/workbook.xml' $WorkbookRels),
            (Get-RelFacts 'xl/worksheets/_rels/sheet1.xml.rels' 'xl/worksheets/sheet1.xml' $SheetRels)
        )
    }
    lying_size = [ordered]@{
        file = 'lying-size.xlsx'
        length = $bombResult.Length
        entry = 'xl/media/zeros.bin'
        declared_size = 4096
        declared_crc32 = (Get-Crc32 (New-Object 'byte[]' 4096))
        expanded_size = 67108864
        compressed_size = $bomb.Payload.Length
    }
}
[System.IO.File]::WriteAllText((Join-Path $OutDir 'manifest.json'), (ConvertTo-Json -InputObject $manifest -Depth 10), $Utf8)
