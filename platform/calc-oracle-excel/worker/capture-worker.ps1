# The capture worker of calc-oracle-excel: the one Excel COM capture implementation of this
# repository. The adapter (src/capture.rs) writes this script into a fresh work directory and starts
# one Windows PowerShell process per capture with a request file; it is not run by hand. Dot-sourced
# (". capture-worker.ps1") it only defines its functions and starts nothing, which is how the
# adapter's unit tests run Read-Package, Get-PackageRefusals and Get-AddInStates without Excel.
#
# It changes no setting that persists beyond its own Excel instance. It sets only the
# Application's Visible, DisplayAlerts, ScreenUpdating, EnableEvents, Interactive and
# AutomationSecurity, which belong to the running instance, and the calculation settings
# (Calculation, Iteration, MaxIterations, MaxChange), which Excel takes from the workbook it opens
# and saves with the workbook it saves (the output package in the work directory). It writes no
# Excel option of the user's profile and no add-in's state, so a timeout that kills it before its
# cleanup leaves the owner's Excel settings as they were.
#
# What it does, in order:
#   1. reads the input package with .NET's ZIP and XML readers (no DTDs, no external resolution):
#      the worksheets, their visibility, every formula cell with its formula type, shared group,
#      range ref and cell metadata index (cm, with whether it names the dynamic-array metadata
#      XLDAPR of the metadata part), each other cell of a legacy array formula's range (an
#      array_member naming its anchor: Excel lists every cell of the range as a formula cell),
#      the data connections with their refreshOnLoad flag,
#      the kind of each external link part, the root element and refreshOnLoad flag of every XML
#      part, and the calls of the formulas it reads to the functions of $ExternalDataFunctions;
#      this inventory comes from the package, not from Excel;
#   2. refuses the capture before Excel starts when the package holds one of the routes to external
#      data or another application listed below: Excel never opens such a workbook, so it never
#      refreshes or activates it;
#   3. starts a new Excel instance through COM and prints its PID, from Application.Hwnd, on
#      standard output before anything else touches it;
#   4. sets the session settings above (DisplayAlerts, ScreenUpdating, EnableEvents and Interactive
#      off, AutomationSecurity 3, force-disable macros), each read back, and reads the state of
#      every add-in (AddIns2: IsOpen and Installed; COMAddIns: Connect; AddIns: Installed) and
#      whether XLL functions are registered. A setting that does not read back as set, an add-in
#      that is open in this instance, an add-in of AddIns2 whose state cannot be read and
#      registered XLL functions refuse the capture as isolation failures, each named by its file
#      name, and Excel quits without the workbook having been opened. A connected COM add-in does
#      not refuse it, and it is not disconnected (the worker writes no add-in's state): on the
#      owner's machine the COM add-in VS15ExcelAdaptor, registered under HKLM with LoadBehavior 3,
#      reads as connected in every automation instance measured. Each connected COM add-in
#      (ProgId and Description) is recorded in the setting com_add_ins_connected, which the
#      verdict counts (calc-verdict), and each installed add-in (its Name, a file name) in
#      add_ins_installed; an add-in installed in the profile but not open in this instance does
#      not refuse the capture. Nothing is switched off;
#   5. opens the input read-only with UpdateLinks 0 (under manual calculation, so Excel does not
#      calculate it on opening, when the request's mode is manual), sets each setting of the
#      calculation policy of the request that Excel does not already hold, forces a full rebuild
#      (with iteration on, a workbook with a circular reference leaves CalculationState pending,
#      which is recorded as the setting calculation_state), and reads each inventory cell's formula (Formula2 only), raw Value2
#      (numbers as their binary64 bits) and spill (Read-Spill: Range.HasSpill, and for a spill the
#      address, rows, columns and Value2 of Range.SpillingToRange), the environment and settings, the
#      workbook's links (an OLE or DDE link is an isolation failure) and its data connections (one
#      that refreshes on open, or whose setting cannot be read, is an isolation failure), and
#      cross-checks Excel's own formula cells against the inventory (a listed cell in the spill
#      range of an inventory cell is a spill member, not an inventory failure). Nothing is ever
#      refreshed;
#   6. saves the workbook to the output path, closes it, reads the output package's caches for the
#      inventory cells and its formulas (type, ref, cm and whether cm names the dynamic-array
#      metadata, against which the conversion checks each spill; an inventory cell without a saved
#      formula is a save failure of that cell), reopens the output read-only and reads each
#      inventory cell's Value2 and spill again;
#   7. quits Excel, writes the result file and prints "done".
#
# The routes to external data or another application that step 2 checks, and no others:
#   - a data connection of the connections part with refreshOnLoad;
#   - a pivot cache definition with refreshOnLoad;
#   - a query table with refreshOnLoad;
#     (pivot cache definitions and query tables are found by the root element of every package
#     part whose first bytes are XML, whatever the part's name or relationship);
#   - an external link part that is not a link to another workbook (an OLE or DDE link, or a part
#     of no known kind);
#   - a call to a function of $ExternalDataFunctions in the formula of a cell of a worksheet,
#     macro sheet or dialog sheet, in a conditional-formatting or data-validation formula of such a
#     sheet (an element f, formula, formula1 or formula2 of any namespace), or in a defined name,
#     matched without regard to case on the function name, with or without _xl prefixes such as
#     _xlfn., outside string literals. FILTERXML over WEBSERVICE is matched through WEBSERVICE.
# A link to another workbook is not refused: Excel opens the workbook with UpdateLinks 0, which
# never refreshes it, and step 5 records it. Any other route (an ActiveX control, an OLE object
# embedded in a sheet, a linked data type, a function of an add-in, a formula part this list does
# not name) is not checked here.
#
# Anything that goes wrong on the way is recorded in the result's failures (or as its fatal error),
# never papered over: every catch below records a failure or refuses the capture, except the
# cleanup in the finally block. Windows PowerShell reads a property whose getter fails as $null
# without an error, so every property the worker reads from Excel's objects goes through
# Get-ComProperty, where a failed COM read throws, and its type is checked, except two collections
# a method is called on at once (Application.Workbooks before Open, Workbook.Worksheets and
# Worksheet.Cells before Item in Read-Cell): a failed read there gives $null, and the method call
# on it throws. The script is ASCII and runs under Windows PowerShell 5.1.

param(
    [string]$Request,
    [string]$Result
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$Protocol = 'dnacalc-capture-worker/1'
$MainNs = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'
$OfficeRelNs = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
$RelPrefix = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/'
$DynamicArrayNs = 'http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray'

function Send-Line([string]$Text) {
    [Console]::Out.WriteLine("$Protocol $Text")
    [Console]::Out.Flush()
}

# Text as 4 lower-case hex digits per UTF-16 code unit, so that any text survives the JSON exactly.
function Get-Units($Text) {
    if ($null -eq $Text) { return $null }
    $builder = New-Object System.Text.StringBuilder
    foreach ($unit in ([string]$Text).ToCharArray()) {
        [void]$builder.Append(([int]$unit).ToString('x4'))
    }
    return $builder.ToString()
}

$Failures = New-Object System.Collections.ArrayList
function Add-Failure([string]$Stage, $Cell, [string]$Message) {
    $entry = [ordered]@{ stage = $Stage }
    if ($null -ne $Cell) {
        $entry.sheet = $Cell.sheet
        $entry.row = $Cell.row
        $entry.column = $Cell.column
    }
    $entry.message = $Message
    [void]$Failures.Add($entry)
}

function Get-Key($Sheet, $Row, $Column) { return "$Sheet|$Row|$Column" }

# --- the package reader -------------------------------------------------------------------------

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

function Find-Entry($Zip, [string]$Name) {
    foreach ($entry in $Zip.Entries) {
        if ($entry.FullName -ieq $Name) { return $entry }
    }
    return $null
}

function Read-Part($Zip, [string]$Name) {
    $entry = Find-Entry $Zip $Name
    if ($null -eq $entry) { return $null }
    $settings = New-Object System.Xml.XmlReaderSettings
    $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
    $settings.XmlResolver = $null
    $stream = $entry.Open()
    try {
        $reader = [System.Xml.XmlReader]::Create($stream, $settings)
        try {
            $document = New-Object System.Xml.XmlDocument
            $document.XmlResolver = $null
            $document.PreserveWhitespace = $true
            $document.Load($reader)
        } finally {
            $reader.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
    # The comma keeps PowerShell from enumerating the document's child nodes into the output.
    return , $document
}

function Resolve-Target([string]$Source, [string]$Target) {
    if ($Target.StartsWith('/')) {
        $path = $Target.Substring(1)
    } else {
        $slash = $Source.LastIndexOf('/')
        $directory = ''
        if ($slash -ge 0) { $directory = $Source.Substring(0, $slash + 1) }
        $path = $directory + $Target
    }
    $parts = New-Object System.Collections.ArrayList
    foreach ($segment in $path.Split('/')) {
        if ($segment -eq '..') {
            if ($parts.Count -gt 0) { $parts.RemoveAt($parts.Count - 1) }
        } elseif ($segment -ne '.' -and $segment -ne '') {
            [void]$parts.Add($segment)
        }
    }
    return ($parts -join '/')
}

function Read-Relationships($Zip, [string]$Part) {
    $slash = $Part.LastIndexOf('/')
    if ($slash -lt 0) {
        $name = "_rels/$Part.rels"
    } else {
        $name = $Part.Substring(0, $slash) + '/_rels/' + $Part.Substring($slash + 1) + '.rels'
    }
    $map = @{}
    $document = Read-Part $Zip $name
    if ($null -eq $document) { return $map }
    foreach ($node in $document.DocumentElement.ChildNodes) {
        if ($node.NodeType -ne [System.Xml.XmlNodeType]::Element -or $node.LocalName -ne 'Relationship') { continue }
        $map[$node.GetAttribute('Id')] = @{
            type = $node.GetAttribute('Type')
            target = $node.GetAttribute('Target')
            external = ($node.GetAttribute('TargetMode') -eq 'External')
        }
    }
    return $map
}

# The 1-based row and column of a relative A1 reference, or $null.
function Split-Reference([string]$Text) {
    if ($Text -cnotmatch '^([A-Z]{1,3})([1-9][0-9]{0,6})$') { return $null }
    $column = 0
    foreach ($letter in $Matches[1].ToCharArray()) { $column = $column * 26 + ([int]$letter - 64) }
    return @{ row = [int]$Matches[2]; column = $column }
}

# The first and last cell of a relative A1 range (B1 or B1:B3), the first neither below nor right
# of the last, or $null.
function Split-Range([string]$Text) {
    $parts = $Text.Split(':')
    if ($parts.Count -gt 2) { return $null }
    $first = Split-Reference $parts[0]
    $last = Split-Reference $parts[$parts.Count - 1]
    if ($null -eq $first -or $null -eq $last -or $last.row -lt $first.row -or $last.column -lt $first.column) { return $null }
    return @{ first = $first; last = $last }
}

# The text of a shared string item or an inline string: its t elements, phonetic runs left out.
function Get-StringText($Node, $Names) {
    $builder = New-Object System.Text.StringBuilder
    foreach ($t in $Node.SelectNodes('x:t | x:r/x:t', $Names)) { [void]$builder.Append($t.InnerText) }
    return $builder.ToString()
}

# The functions whose call in a formula reaches external data or another application, by the name
# a formula's text gives them after any _xl prefix (_xlfn., _xlws.): real-time data (RTD), web
# requests and web images (WEBSERVICE, IMAGE), stock data (STOCKHISTORY), OLAP cube data (the CUBE
# functions), the Microsoft services behind TRANSLATE, DETECTLANGUAGE, COPILOT and PY (Python in
# Excel), file imports (IMPORTTEXT, IMPORTCSV), and calls into a DLL or an ODBC source (CALL,
# REGISTER.ID, SQL.REQUEST).
$ExternalDataFunctions = @(
    'RTD', 'WEBSERVICE', 'IMAGE', 'STOCKHISTORY',
    'CUBEKPIMEMBER', 'CUBEMEMBER', 'CUBEMEMBERPROPERTY', 'CUBERANKEDMEMBER', 'CUBESET', 'CUBESETCOUNT', 'CUBEVALUE',
    'TRANSLATE', 'DETECTLANGUAGE', 'COPILOT', 'PY', 'IMPORTTEXT', 'IMPORTCSV',
    'CALL', 'REGISTER.ID', 'SQL.REQUEST'
)
# A call: one of the names, after any _xl prefixes, not preceded by a character a longer name could
# end with, and followed by an opening parenthesis.
$ExternalCallPattern = New-Object System.Text.RegularExpressions.Regex(
    ('(?<![\p{L}\p{N}_.\\?])(?:_xl[a-z]+\.)*(' + (($ExternalDataFunctions | ForEach-Object { [System.Text.RegularExpressions.Regex]::Escape($_) }) -join '|') + ')\s*\('),
    [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
# A formula's string literal, whose doubled quotes stand for one.
$StringLiteralPattern = New-Object System.Text.RegularExpressions.Regex('"(?:[^"]|"")*"')

# The functions of $ExternalDataFunctions that a formula's text calls outside its string literals,
# each once, in upper case.
function Get-ExternalCalls([string]$Formula) {
    $found = New-Object System.Collections.ArrayList
    $code = $StringLiteralPattern.Replace($Formula, '""')
    foreach ($match in $ExternalCallPattern.Matches($code)) {
        $name = $match.Groups[1].Value.ToUpperInvariant()
        if (-not $found.Contains($name)) { [void]$found.Add($name) }
    }
    return , $found
}

# The root element of a package part whose first bytes are XML (an optional UTF-8 or UTF-16 byte
# order mark, optional white space, then "<"), with its refreshOnLoad flag, or $null for a part
# that is not XML (a VBA project, an image).
function Read-Root($Entry) {
    $head = New-Object byte[] 64
    $count = 0
    $stream = $Entry.Open()
    try {
        while ($count -lt $head.Length) {
            $read = $stream.Read($head, $count, $head.Length - $count)
            if ($read -le 0) { break }
            $count += $read
        }
    } finally {
        $stream.Dispose()
    }
    $at = 0
    $unit = 1
    if ($count -ge 3 -and $head[0] -eq 0xEF -and $head[1] -eq 0xBB -and $head[2] -eq 0xBF) { $at = 3 }
    elseif ($count -ge 2 -and $head[0] -eq 0xFF -and $head[1] -eq 0xFE) { $at = 2; $unit = 2 }
    elseif ($count -ge 2 -and $head[0] -eq 0xFE -and $head[1] -eq 0xFF) { $at = 3; $unit = 2 }
    while ($at -lt $count -and @(0x20, 0x09, 0x0D, 0x0A) -contains $head[$at]) { $at += $unit }
    if ($at -ge $count -or $head[$at] -ne 0x3C) { return $null }
    $settings = New-Object System.Xml.XmlReaderSettings
    $settings.DtdProcessing = [System.Xml.DtdProcessing]::Prohibit
    $settings.XmlResolver = $null
    $stream = $Entry.Open()
    try {
        $reader = [System.Xml.XmlReader]::Create($stream, $settings)
        try {
            [void]$reader.MoveToContent()
            $refresh = $reader.GetAttribute('refreshOnLoad')
            return [ordered]@{ part = $Entry.FullName; element = $reader.LocalName; refresh_on_load = ($refresh -eq '1' -or $refresh -eq 'true') }
        } finally {
            $reader.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

# The cell metadata blocks of a metadata part (cellMetadata/bk, in order: a cell's cm="n" names the
# n-th), each $true when one of its records (rc: t, the 1-based index of a metadataType, and v, the
# 0-based index of a block of that type's futureMetadata) names the type XLDAPR and a block whose
# dynamicArrayProperties has fDynamic set, which is how Excel with dynamic arrays marks a
# dynamic-array formula, and $false otherwise.
function Read-DynamicArrayBlocks($Document) {
    $names = New-Object System.Xml.XmlNamespaceManager($Document.NameTable)
    $names.AddNamespace('x', $MainNs)
    $types = @(foreach ($type in $Document.SelectNodes('/x:metadata/x:metadataTypes/x:metadataType', $names)) { $type.GetAttribute('name') })
    $dynamic = @(foreach ($bk in $Document.SelectNodes("/x:metadata/x:futureMetadata[@name='XLDAPR']/x:bk", $names)) {
        $properties = $bk.SelectSingleNode(".//*[local-name()='dynamicArrayProperties' and namespace-uri()='$DynamicArrayNs']")
        ($null -ne $properties) -and (@('1', 'true') -contains $properties.GetAttribute('fDynamic'))
    })
    $blocks = New-Object System.Collections.ArrayList
    foreach ($bk in $Document.SelectNodes('/x:metadata/x:cellMetadata/x:bk', $names)) {
        $marked = $false
        foreach ($rc in $bk.SelectNodes('x:rc', $names)) {
            $t = Get-Index $rc.GetAttribute('t')
            $v = Get-Index $rc.GetAttribute('v')
            if ($null -ne $t -and $null -ne $v -and $t -ge 1 -and $t -le $types.Count -and $types[$t - 1] -eq 'XLDAPR' -and $v -lt $dynamic.Count -and $dynamic[$v]) { $marked = $true }
        }
        [void]$blocks.Add($marked)
    }
    return , $blocks
}

# An index written as decimal digits only, or $null.
function Get-Index([string]$Text) {
    $index = 0
    if ([int]::TryParse($Text, [System.Globalization.NumberStyles]::None, [System.Globalization.CultureInfo]::InvariantCulture, [ref]$index)) { return $index }
    return $null
}

# The worksheets of a package with their visibility, every cell that holds a formula or a value
# (formula type, shared group, ref and cell metadata index cm with whether cm names the
# dynamic-array metadata (Read-DynamicArrayBlocks), and the cached value's type and text), each
# other cell of a legacy array formula's range as a formula of type array_member with its anchor, the
# data connections of its connections part with their refreshOnLoad flag, the kind of
# each external link part (the local name of its first element: externalBook, ddeLink or oleLink),
# the root element of every XML part with its refreshOnLoad flag, and each formula of a sheet (cell,
# conditional formatting, data validation) or defined name that calls a function of
# $ExternalDataFunctions.
function Read-Package([string]$Path) {
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Path)
    try {
        $root = Read-Relationships $zip ''
        $workbookPart = $null
        foreach ($rel in $root.Values) {
            if ($rel.type -eq ($RelPrefix + 'officeDocument')) { $workbookPart = Resolve-Target '' $rel.target }
        }
        if ($null -eq $workbookPart) { throw 'the package has no officeDocument relationship' }
        $workbook = Read-Part $zip $workbookPart
        if ($null -eq $workbook -or $workbook.DocumentElement.NamespaceURI -ne $MainNs) {
            throw "the workbook part $workbookPart is not transitional SpreadsheetML"
        }
        $names = New-Object System.Xml.XmlNamespaceManager($workbook.NameTable)
        $names.AddNamespace('x', $MainNs)
        $rels = Read-Relationships $zip $workbookPart

        $strings = New-Object System.Collections.ArrayList
        $connections = New-Object System.Collections.ArrayList
        $links = New-Object System.Collections.ArrayList
        $cellMetadata = New-Object System.Collections.ArrayList
        foreach ($id in ($rels.Keys | Sort-Object)) {
            $rel = $rels[$id]
            if ($rel.type -eq ($RelPrefix + 'sharedStrings')) {
                $shared = Read-Part $zip (Resolve-Target $workbookPart $rel.target)
                if ($null -eq $shared) { continue }
                $sharedNames = New-Object System.Xml.XmlNamespaceManager($shared.NameTable)
                $sharedNames.AddNamespace('x', $MainNs)
                foreach ($item in $shared.SelectNodes('/x:sst/x:si', $sharedNames)) {
                    [void]$strings.Add((Get-StringText $item $sharedNames))
                }
            } elseif ($rel.type -eq ($RelPrefix + 'connections')) {
                $part = Resolve-Target $workbookPart $rel.target
                $document = Read-Part $zip $part
                if ($null -eq $document) { throw "the connections part $part is missing" }
                $connectionNames = New-Object System.Xml.XmlNamespaceManager($document.NameTable)
                $connectionNames.AddNamespace('x', $MainNs)
                foreach ($connection in $document.SelectNodes('/x:connections/x:connection', $connectionNames)) {
                    $refresh = $connection.GetAttribute('refreshOnLoad')
                    [void]$connections.Add([ordered]@{
                        name = $connection.GetAttribute('name')
                        type = $connection.GetAttribute('type')
                        refresh_on_load = ($refresh -eq '1' -or $refresh -eq 'true')
                    })
                }
            } elseif ($rel.type -eq ($RelPrefix + 'externalLink')) {
                $part = Resolve-Target $workbookPart $rel.target
                $document = Read-Part $zip $part
                if ($null -eq $document) { throw "the external link part $part is missing" }
                $kind = 'empty'
                foreach ($node in $document.DocumentElement.ChildNodes) {
                    if ($node.NodeType -eq [System.Xml.XmlNodeType]::Element -and $node.LocalName -ne 'extLst') {
                        $kind = $node.LocalName
                        break
                    }
                }
                [void]$links.Add([ordered]@{ part = $part; kind = $kind })
            } elseif ($rel.type -eq ($RelPrefix + 'sheetMetadata')) {
                $part = Resolve-Target $workbookPart $rel.target
                $document = Read-Part $zip $part
                if ($null -eq $document) { throw "the metadata part $part is missing" }
                $cellMetadata = Read-DynamicArrayBlocks $document
            }
        }

        $sheets = New-Object System.Collections.ArrayList
        $formulas = New-Object System.Collections.ArrayList
        $caches = @{}
        $externalCalls = New-Object System.Collections.ArrayList
        foreach ($sheet in $workbook.SelectNodes('/x:workbook/x:sheets/x:sheet', $names)) {
            $name = $sheet.GetAttribute('name')
            $rel = $rels[$sheet.GetAttribute('id', $OfficeRelNs)]
            if ($null -eq $rel) { throw "sheet $name has no relationship" }
            if ($rel.type -eq ($RelPrefix + 'chartsheet')) { continue }
            $part = Resolve-Target $workbookPart $rel.target
            $document = Read-Part $zip $part
            if ($null -eq $document) { throw "the part $part of sheet $name is missing" }
            # The formulas of every sheet with cells (worksheets, macro sheets, dialog sheets): those
            # of its cells and of its conditional-formatting and data-validation rules, in any
            # namespace (the x14 extensions hold theirs in xm:f).
            foreach ($node in $document.SelectNodes("//*[local-name()='f' or local-name()='formula' or local-name()='formula1' or local-name()='formula2']")) {
                $called = Get-ExternalCalls $node.InnerText
                if ($called.Count -eq 0) { continue }
                $parent = $node.ParentNode
                if ($parent.LocalName -eq 'c') {
                    $where = "the formula of $name!" + $parent.GetAttribute('r')
                } else {
                    $where = 'a ' + $parent.LocalName + ' ' + $node.LocalName + " of sheet $name"
                }
                [void]$externalCalls.Add([ordered]@{ where = $where; functions = @($called) })
            }
            # An Excel 4.0 macro sheet (dc-xuw) is inventoried as a worksheet is: its document element
            # is xm:macrosheet, in Microsoft's namespace, and its relationship type is in Microsoft's
            # namespace too, not the one $RelPrefix names. Its formula cells are inventory cells,
            # which DNA Calc does not calculate.
            $macroSheet = $rel.type -eq 'http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet' -or $rel.type -eq 'http://schemas.microsoft.com/office/2006/relationships/xlIntlMacrosheet'
            if ($rel.type -ne ($RelPrefix + 'worksheet') -and -not $macroSheet) {
                # Dialog sheets.
                $kind = $rel.type.Substring($rel.type.LastIndexOf('/') + 1)
                Add-Failure 'inventory' $null ("sheet $name is a $kind (relationship type " + $rel.type + '), whose cells are not captured')
                continue
            }
            switch ($sheet.GetAttribute('state')) {
                'hidden' { $visibility = 'hidden' }
                'veryHidden' { $visibility = 'very_hidden' }
                default { $visibility = 'visible' }
            }
            [void]$sheets.Add([ordered]@{ name = $name; visibility = $visibility })
            $sheetNames = New-Object System.Xml.XmlNamespaceManager($document.NameTable)
            $sheetNames.AddNamespace('x', $MainNs)
            $sheetNames.AddNamespace('xm', 'http://schemas.microsoft.com/office/excel/2006/main')
            $sheetStart = $formulas.Count
            $cellPath = if ($macroSheet) { '/xm:macrosheet/x:sheetData/x:row/x:c' } else { '/x:worksheet/x:sheetData/x:row/x:c' }
            foreach ($c in $document.SelectNodes($cellPath, $sheetNames)) {
                $place = Split-Reference $c.GetAttribute('r')
                if ($null -eq $place) {
                    Add-Failure 'inventory' $null ("sheet $name has a cell whose reference '" + $c.GetAttribute('r') + "' is not a relative A1 reference")
                    continue
                }
                $key = Get-Key $name $place.row $place.column
                $f = $c.SelectSingleNode('x:f', $sheetNames)
                if ($null -ne $f) {
                    $type = $f.GetAttribute('t')
                    if ($type -eq '') { $type = 'normal' }
                    $entry = [ordered]@{ sheet = $name; row = $place.row; column = $place.column; type = $type; si = $null; ref = $null; cm = $null; dynamic_array_metadata = $false }
                    if ($f.HasAttribute('si')) { $entry.si = [int64]$f.GetAttribute('si') }
                    if ($f.HasAttribute('ref')) { $entry.ref = $f.GetAttribute('ref') }
                    if ($c.HasAttribute('cm')) {
                        $entry.cm = $c.GetAttribute('cm')
                        $block = Get-Index $entry.cm
                        if ($null -ne $block -and $block -ge 1 -and $block -le $cellMetadata.Count) { $entry.dynamic_array_metadata = [bool]$cellMetadata[$block - 1] }
                    }
                    [void]$formulas.Add($entry)
                }
                $t = $c.GetAttribute('t')
                if ($t -eq '') { $t = 'n' }
                $v = $c.SelectSingleNode('x:v', $sheetNames)
                if ($t -eq 'inlineStr') {
                    $is = $c.SelectSingleNode('x:is', $sheetNames)
                    if ($null -ne $is) { $caches[$key] = @{ t = 'str'; v = (Get-StringText $is $sheetNames) } }
                } elseif ($null -ne $v) {
                    $text = $v.InnerText
                    if ($t -eq 's') {
                        $index = [int]$text
                        if ($index -lt 0 -or $index -ge $strings.Count) { throw "cell $name!$($c.GetAttribute('r')) names shared string $index, which the package does not hold" }
                        $caches[$key] = @{ t = 'str'; v = $strings[$index] }
                    } else {
                        $caches[$key] = @{ t = $t; v = $text }
                    }
                }
            }
            # The members of each legacy array formula of the sheet (type array whose cell metadata
            # does not name the dynamic-array metadata XLDAPR, with a ref starting at its cell). The
            # package holds the formula on its anchor only, while Excel lists every cell of the
            # range among its formula cells, each showing its own element, so each other cell of
            # the range is a formula cell of the inventory too: listed after the sheet's own
            # formulas, in rows, as type array_member with the anchor's reference. A cell of the
            # range that holds a formula of its own or lies in the range of an earlier legacy array
            # is an inventory failure, and is not listed twice. A legacy array whose ref is not a
            # range starting at its cell gets no members; the conversion refuses it.
            # The cells of a what-if data table's result area (type dataTable on its top-left cell,
            # dc-10y) are listed the same way, as type data_table_member: Excel lists each among
            # its formula cells too.
            $sheetEntries = @($formulas.GetRange($sheetStart, $formulas.Count - $sheetStart))
            $held = @{}
            foreach ($entry in $sheetEntries) { $held[(Get-Key $name $entry.row $entry.column)] = $true }
            foreach ($entry in $sheetEntries) {
                if ($entry.type -eq 'dataTable') {
                    $memberType = 'data_table_member'
                    $what = 'data table'
                } elseif ($entry.type -eq 'array' -and -not $entry.dynamic_array_metadata) {
                    $memberType = 'array_member'
                    $what = 'legacy array formula'
                } else {
                    continue
                }
                if ($null -eq $entry.ref) { continue }
                $range = Split-Range $entry.ref
                if ($null -eq $range -or $range.first.row -ne $entry.row -or $range.first.column -ne $entry.column) { continue }
                $anchor = $entry.ref.Split(':')[0]
                for ($row = $range.first.row; $row -le $range.last.row; $row++) {
                    for ($column = $range.first.column; $column -le $range.last.column; $column++) {
                        if ($row -eq $entry.row -and $column -eq $entry.column) { continue }
                        $key = Get-Key $name $row $column
                        if ($held.ContainsKey($key)) {
                            Add-Failure 'inventory' ([ordered]@{ sheet = $name; row = $row; column = $column }) ("the cell lies in the range " + $entry.ref + " of the $what at $name!$anchor and holds a formula of its own or lies in the range of another legacy array formula or data table")
                            continue
                        }
                        $held[$key] = $true
                        [void]$formulas.Add([ordered]@{ sheet = $name; row = $row; column = $column; type = $memberType; si = $null; ref = $null; cm = $null; dynamic_array_metadata = $false; anchor = $anchor })
                    }
                }
            }
        }
        foreach ($defined in $workbook.SelectNodes('/x:workbook/x:definedNames/x:definedName', $names)) {
            $called = Get-ExternalCalls $defined.InnerText
            if ($called.Count -gt 0) {
                [void]$externalCalls.Add([ordered]@{ where = 'the defined name ' + $defined.GetAttribute('name'); functions = @($called) })
            }
        }
        $roots = New-Object System.Collections.ArrayList
        foreach ($entry in $zip.Entries) {
            if ($entry.FullName.EndsWith('/')) { continue }
            $root = Read-Root $entry
            if ($null -ne $root) { [void]$roots.Add($root) }
        }
        return @{ sheets = $sheets; formulas = $formulas; caches = $caches; connections = $connections; links = $links; roots = $roots; external_calls = $externalCalls }
    } finally {
        $zip.Dispose()
    }
}

# Why Excel must not open a package at all (the routes the header lists): each data connection,
# pivot cache definition and query table that refreshes on load, each external link that is not a
# link to another workbook (an OLE or DDE link, or a link part of no known kind), and each formula
# that calls a function of $ExternalDataFunctions. Excel can refresh such a connection, cache or
# query table while it opens the workbook, an OLE or DDE link names another application to start
# or talk to, and the full rebuild would call such a function, so none is ever let near Excel.
function Get-PackageRefusals($Package) {
    $refusals = New-Object System.Collections.ArrayList
    foreach ($connection in $Package.connections) {
        if ($connection.refresh_on_load) { [void]$refusals.Add("data connection '" + $connection.name + "' refreshes on load") }
    }
    foreach ($root in $Package.roots) {
        if (-not $root.refresh_on_load) { continue }
        if ($root.element -eq 'pivotCacheDefinition') { [void]$refusals.Add('pivot cache definition ' + $root.part + ' refreshes on load') }
        if ($root.element -eq 'queryTable') { [void]$refusals.Add('query table ' + $root.part + ' refreshes on load') }
    }
    foreach ($link in $Package.links) {
        if ($link.kind -ne 'externalBook') { [void]$refusals.Add('external link part ' + $link.part + ' is a ' + $link.kind + ' link') }
    }
    foreach ($call in $Package.external_calls) {
        [void]$refusals.Add($call.where + ' calls ' + ($call.functions -join ', ') + ', which reaches external data or another application')
    }
    return , $refusals
}

# --- the Excel session --------------------------------------------------------------------------

Add-Type -Namespace DnaCalcCapture -Name NativeMethods -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint processId);
'@

# The worker's COM message filter: a call Excel rejects because it is busy
# (SERVERCALL_RETRYLATER, which PowerShell reports as RPC_E_CALL_REJECTED) is sent again every
# 100 ms for up to ten minutes, and then fails as before. Excel 16.0.20527 rejected calls right
# after the catalogue oracle's CalculateFullRebuild had reported done (2026-10-29); the retry
# changes no value read, only when it is read. A call rejected for another reason fails at once.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace DnaCalcCapture {
    [ComImport, Guid("00000016-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IOleMessageFilter {
        [PreserveSig] int HandleInComingCall(int callType, IntPtr caller, int tickCount, IntPtr interfaceInfo);
        [PreserveSig] int RetryRejectedCall(IntPtr callee, int tickCount, int rejectType);
        [PreserveSig] int MessagePending(IntPtr callee, int tickCount, int pendingType);
    }
    public sealed class BusyRetryFilter : IOleMessageFilter {
        [DllImport("ole32.dll")]
        private static extern int CoRegisterMessageFilter(IOleMessageFilter filter, out IOleMessageFilter previous);
        public static int Register() {
            IOleMessageFilter previous;
            return CoRegisterMessageFilter(new BusyRetryFilter(), out previous);
        }
        public int HandleInComingCall(int callType, IntPtr caller, int tickCount, IntPtr interfaceInfo) { return 0; }
        public int RetryRejectedCall(IntPtr callee, int tickCount, int rejectType) {
            return (rejectType == 2 && tickCount < 600000) ? 100 : -1;
        }
        public int MessagePending(IntPtr callee, int tickCount, int pendingType) { return 2; }
    }
}
'@

function Get-ExcelPid($Application) {
    $handle = Get-ComProperty $Application 'Hwnd'
    if (-not ($handle -is [int] -or $handle -is [long])) { throw ('Application.Hwnd reads as ' + (Get-Shown $handle) + ', not a window handle') }
    $window = [System.IntPtr]::new([int64]$handle)
    $processId = [uint32]0
    [void][DnaCalcCapture.NativeMethods]::GetWindowThreadProcessId($window, [ref]$processId)
    if ($processId -eq 0) { throw "Excel's window handle names no process" }
    return [int]$processId
}

# A raw Value2 as the canonical typed value: numbers as their binary64 bits, text as UTF-16 units,
# a worksheet error as Excel's error code (the low 16 bits of the VT_ERROR value).
function Convert-Value($Value) {
    if ($null -eq $Value) { return [ordered]@{ empty = $true } }
    if ($Value -is [double]) { return [ordered]@{ number = [BitConverter]::DoubleToInt64Bits($Value).ToString('x16') } }
    if ($Value -is [string]) { return [ordered]@{ text = (Get-Units $Value) } }
    if ($Value -is [bool]) { return [ordered]@{ logical = $Value } }
    if ($Value -is [int]) { return [ordered]@{ error_code = ($Value -band 0xFFFF) } }
    return [ordered]@{ unsupported = $Value.GetType().FullName }
}

function Get-Machine([string]$Path) {
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $reader = New-Object System.IO.BinaryReader($stream)
        $stream.Position = 0x3C
        $stream.Position = $reader.ReadInt32() + 4
        $machine = $reader.ReadUInt16()
    } finally {
        $stream.Dispose()
    }
    switch ($machine) {
        0x8664 { return 'x86-64' }
        0x014C { return 'x86' }
        0xAA64 { return 'arm64' }
        default { return 'machine-' + ([int]$machine).ToString('x4') }
    }
}

function Get-Channel {
    $key = 'HKLM:\SOFTWARE\Microsoft\Office\ClickToRun\Configuration'
    try {
        $configuration = Get-ItemProperty -LiteralPath $key
    } catch {
        Add-Failure 'environment' $null 'no Click-to-Run configuration: the update channel is unknown'
        return @('unknown', '')
    }
    $source = ''
    foreach ($name in @('UpdateChannel', 'CDNBaseUrl')) {
        if ($null -ne $configuration.PSObject.Properties[$name]) {
            $source = [string]$configuration.$name
            if ($source -ne '') { break }
        }
    }
    $channels = @{
        '492350f6-3a01-4f97-b9c0-c7c6ddf67d60' = 'current'
        '64256afe-f5d9-4f86-8936-8840a6a4f5be' = 'current-preview'
        '5440fd1f-7ecb-4221-8110-145efaa6372f' = 'beta'
        '55336b82-a18d-4dd6-b5f6-9e5095c314a6' = 'monthly-enterprise'
        '7ffbc6bf-bc32-4f92-8982-f9dd17fd3114' = 'semi-annual-enterprise'
        'b8f9b850-328d-4355-9145-c59439a0c4cf' = 'semi-annual-enterprise-preview'
    }
    $guid = $source.TrimEnd('/').Split('/')[-1].ToLowerInvariant()
    if ($channels.ContainsKey($guid)) { return @($channels[$guid], $source) }
    if ($guid -ne '') { return @("cdn-$guid", $source) }
    Add-Failure 'environment' $null 'the Click-to-Run configuration names no update channel'
    return @('unknown', $source)
}

$Missing = [System.Type]::Missing
$NoPassword = 'dnacalc-no-password'

function Open-Workbook($Application, [string]$Path) {
    # FileName, UpdateLinks 0 (never), ReadOnly, Format, Password, WriteResPassword,
    # IgnoreReadOnlyRecommended, Origin, Delimiter, Editable, Notify, Converter, AddToMru, Local,
    # CorruptLoad 0 (normal load, no repair). A password that cannot be the workbook's makes an
    # encrypted workbook fail to open instead of prompting. WriteResPassword is left out: Excel
    # 16.0.20430 failed Open of replay.xlsx, which has no write reservation, given one ("Open
    # method of Workbooks class failed"). A write-reserved workbook given none opened read-only
    # without a prompt (16.0.20430, this function and the session settings above: ReadOnly True,
    # in 0.3 s). The call goes through Invoke-ComMethod: Windows PowerShell's own call of Open with
    # these arguments failed on that build ("Unable to get the Open property of the Workbooks
    # class"). $Path is a [string], not a PSObject wrapping one: InvokeMember hands a wrapped
    # string to COM as an object Open refuses (DISP_E_TYPEMISMATCH, measured the same way).
    return (Invoke-ComMethod $Application.Workbooks 'Open' @($Path, 0, $true, $Missing, $NoPassword, $Missing, $true, $Missing, $Missing, $false, $false, $Missing, $false, $false, 0))
}

# A method of a COM object called through COM's own call (InvokeMember), which passes
# [System.Type]::Missing as an omitted argument. Any other object (the unit tests' stand-in
# Application) has the method called as a PowerShell method.
function Invoke-ComMethod($Object, [string]$Name, [object[]]$Arguments) {
    if ($Object -is [System.__ComObject]) {
        return , [System.__ComObject].InvokeMember($Name, [System.Reflection.BindingFlags]::InvokeMethod, $null, $Object, $Arguments)
    }
    return , $Object.$Name.Invoke($Arguments)
}

function Read-Cell($Workbook, $Entry) {
    # Sheets, not Worksheets: an inventory cell may be on a macro sheet (dc-xuw).
    return $Workbook.Sheets.Item($Entry.sheet).Cells.Item($Entry.row, $Entry.column)
}

# A property of a COM object, read so that a failed read throws; $Arguments are the index of a
# property that takes one (Application.International(i)). Windows PowerShell's own property read
# ($object.Name) turns a getter that fails into $null, for COM objects as for .NET ones, which
# would then read as a value (a flag as off, a Value2 as empty). Any other object (the unit tests'
# stand-in Application) is read as a PowerShell property, or its indexed property called as a
# method, whose failed getter gives $null too; every caller therefore also checks the type of
# what it reads.
function Get-ComProperty($Object, [string]$Name, [object[]]$Arguments = $null) {
    if ($Object -is [System.__ComObject]) {
        return , [System.__ComObject].InvokeMember($Name, [System.Reflection.BindingFlags]::GetProperty, $null, $Object, $Arguments)
    }
    if ($null -ne $Arguments) { return , $Object.$Name.Invoke($Arguments) }
    return , $Object.$Name
}

# How a value read is shown in a message: its text and .NET type, or "nothing".
function Get-Shown($Value) {
    if ($null -eq $Value) { return 'nothing' }
    return "$Value (" + $Value.GetType().Name + ')'
}

# A boolean property read through Get-ComProperty; a failed read, or one that is not a boolean,
# throws.
function Get-Flag($Object, [string]$Name) {
    $value = Get-ComProperty $Object $Name
    if ($value -isnot [bool]) { throw ("$Name reads as " + (Get-Shown $value) + ', not a boolean') }
    return $value
}

# The raw Value2 of a cell (Convert-Value), or why it cannot be read.
function Read-Value($Cell) {
    try {
        $value = Get-ComProperty $Cell 'Value2'
    } catch {
        return [ordered]@{ unread = $_.Exception.Message }
    }
    return (Convert-Value $value)
}

# Whether a formula cell spills, read through Get-ComProperty: $null when Range.HasSpill reads as
# False; the relative A1 address (Address(False, False)), Rows.Count and Columns.Count of
# Range.SpillingToRange when it reads as True, with the range's Value2 as values (each element
# through Convert-Value, in row-major order: the array the formula returns, dc-677); or, as unread,
# why that cannot be read (a read that fails or gives another type, a SpillingToRange that reads as
# nothing, a Value2 that is not an array of that shape). A spilling cell's own Value2 is only the
# top-left value of its array, so the conversion records the spill's values as the cell's
# observation, never that value, and refuses the capture when a spill cannot be read.
function Read-Spill($Cell) {
    try {
        $has = Get-ComProperty $Cell 'HasSpill'
        if ($has -isnot [bool]) { throw ('Range.HasSpill reads as ' + (Get-Shown $has) + ', not a boolean') }
        if (-not $has) { return $null }
        $range = Get-ComProperty $Cell 'SpillingToRange'
        if ($null -eq $range) { throw 'Range.HasSpill reads as True and Range.SpillingToRange as nothing' }
        $address = Get-ComProperty $range 'Address' @($false, $false)
        if ($address -isnot [string]) { throw ('Range.SpillingToRange.Address reads as ' + (Get-Shown $address) + ', not a text') }
        $rows = Get-ComProperty (Get-ComProperty $range 'Rows') 'Count'
        $columns = Get-ComProperty (Get-ComProperty $range 'Columns') 'Count'
        if (-not ($rows -is [int] -and $columns -is [int])) {
            throw ('the Rows.Count and Columns.Count of Range.SpillingToRange read as ' + (Get-Shown $rows) + ' and ' + (Get-Shown $columns))
        }
        $array = Get-ComProperty $range 'Value2'
        if (-not ($array -is [System.Array] -and $array.Rank -eq 2 -and $array.GetLength(0) -eq $rows -and $array.GetLength(1) -eq $columns)) {
            throw ('the Value2 of Range.SpillingToRange reads as ' + (Get-Shown $array) + ', not an array of ' + $rows + ' rows and ' + $columns + ' columns')
        }
        $values = New-Object System.Collections.ArrayList
        $top = $array.GetLowerBound(0); $left = $array.GetLowerBound(1)
        for ($r = 0; $r -lt $rows; $r++) {
            for ($c = 0; $c -lt $columns; $c++) { [void]$values.Add((Convert-Value $array[($top + $r), ($left + $c)])) }
        }
        return [ordered]@{ ref = $address; rows = $rows; columns = $columns; values = $values }
    } catch {
        return [ordered]@{ unread = $_.Exception.Message }
    }
}

# One field of an item of a setting that lists several things (add-ins, links, connections): such a
# setting separates its items with ';' and an item's fields with '|', so a field writes '%', ';'
# and '|' as %25, %3B and %7C, and a name that holds a separator stays one field of one item
# (calc-verdict's ExcelEnvironment::settings states the same rule).
function Get-ListField([string]$Text) {
    return $Text.Replace('%', '%25').Replace(';', '%3B').Replace('|', '%7C')
}

# The last part of a path or name, after its last backslash or '/' (split by hand, so a name with
# characters .NET's path functions refuse is still split): how the record and the refusals name an
# add-in or a linked workbook, never by a path.
function Get-LeafName([string]$Path) {
    $cut = $Path.LastIndexOfAny([char[]]@([char]'\', [char]'/'))
    if ($cut -ge 0) { return $Path.Substring($cut + 1) }
    return $Path
}

# The add-ins of an Excel instance, read and never changed (no add-in is switched off or on, and
# no setting is written). Every add-in is named by its ProgId or its file name, never by a path.
# In loaded, what refuses the capture: each add-in that is open in this instance (AddIns2: IsOpen;
# one installed in the profile but not open is recorded only), each add-in or list of AddIns2
# whose state cannot be read, and registered XLL functions, every one named. A connected COM
# add-in does not refuse the capture and is not disconnected; it is recorded, and the verdict
# counts it (bead dc-quhd owns capturing without one). In connected, each COM add-in whose Connect
# reads as True, as "ProgId|Description" (COMAddIns); in installed, each add-in of
# Application.AddIns whose Installed reads as True, by its Name (a file name, its last part
# through Get-LeafName). Each field goes through Get-ListField. A COM add-in or an installed
# add-in whose state or names cannot be read is an environment failure (the record then says it
# is incomplete), not a refusal. In states, the state read of every COM add-in and every add-in
# of AddIns2, by ProgId or file name.
function Get-AddInStates($Application) {
    $loaded = New-Object System.Collections.ArrayList
    $states = New-Object System.Collections.ArrayList
    $connectedAddIns = New-Object System.Collections.ArrayList
    $installedAddIns = New-Object System.Collections.ArrayList
    try {
        $comAddIns = Get-ComProperty $Application 'COMAddIns'
        if ($null -eq $comAddIns) { throw 'COMAddIns reads as nothing' }
        $index = 0
        foreach ($comAddIn in $comAddIns) {
            $index += 1
            $label = "COM add-in $index"
            try {
                $progId = Get-ComProperty $comAddIn 'ProgId'
                if ($progId -isnot [string] -or $progId -eq '') { throw ('its ProgId reads as ' + (Get-Shown $progId)) }
                $label = "COM add-in $progId"
                $isConnected = Get-Flag $comAddIn 'Connect'
                [void]$states.Add('COM add-in ' + (Get-ListField $progId) + ' connect=' + ([string]$isConnected).ToLowerInvariant())
                if ($isConnected) {
                    $description = Get-ComProperty $comAddIn 'Description'
                    if ($description -isnot [string]) { throw ('its Description reads as ' + (Get-Shown $description)) }
                    [void]$connectedAddIns.Add((Get-ListField $progId) + '|' + (Get-ListField $description))
                }
            } catch {
                Add-Failure 'environment' $null "the state of $label cannot be read, so the record may omit it from com_add_ins_connected: $($_.Exception.Message)"
            }
        }
    } catch {
        Add-Failure 'environment' $null "the COM add-ins cannot be listed, so com_add_ins_connected may be incomplete: $($_.Exception.Message)"
    }
    try {
        $managed = Get-ComProperty $Application 'AddIns'
        if ($null -eq $managed) { throw 'AddIns reads as nothing' }
        $index = 0
        foreach ($addIn in $managed) {
            $index += 1
            try {
                $isInstalled = Get-Flag $addIn 'Installed'
                if ($isInstalled) {
                    $name = Get-ComProperty $addIn 'Name'
                    if ($name -isnot [string] -or $name -eq '') { throw ('its Name reads as ' + (Get-Shown $name)) }
                    [void]$installedAddIns.Add((Get-ListField (Get-LeafName $name)))
                }
            } catch {
                Add-Failure 'environment' $null "the state of installed add-in $index (Application.AddIns) cannot be read, so the record may omit it from add_ins_installed: $($_.Exception.Message)"
            }
        }
    } catch {
        Add-Failure 'environment' $null "the installed add-ins (Application.AddIns) cannot be listed, so add_ins_installed may be incomplete: $($_.Exception.Message)"
    }
    try {
        $addIns = Get-ComProperty $Application 'AddIns2'
        if ($null -eq $addIns) { throw 'AddIns2 reads as nothing' }
        $index = 0
        foreach ($addIn in $addIns) {
            $index += 1
            $label = "add-in $index"
            $shown = $label
            try {
                $fullName = Get-ComProperty $addIn 'FullName'
                if ($fullName -is [string] -and $fullName -ne '') {
                    $leaf = Get-LeafName $fullName
                    $label = "add-in $leaf"
                    $shown = 'add-in ' + (Get-ListField $leaf)
                }
                $open = Get-Flag $addIn 'IsOpen'
                $installed = Get-Flag $addIn 'Installed'
                [void]$states.Add("$shown open=" + ([string]$open).ToLowerInvariant() + ' installed=' + ([string]$installed).ToLowerInvariant())
                if ($open) { [void]$loaded.Add("$label is open") }
            } catch {
                [void]$loaded.Add("the state of $label cannot be read: $($_.Exception.Message)")
            }
        }
    } catch {
        [void]$loaded.Add("the add-ins cannot be listed: $($_.Exception.Message)")
    }
    try {
        # Excel documents Null when no function is registered (a COM Null, read as DBNull); a read
        # that fails throws through Get-ComProperty.
        $registered = Get-ComProperty $Application 'RegisteredFunctions'
        if ($null -ne $registered -and $registered -isnot [System.DBNull]) {
            [void]$loaded.Add('XLL functions are registered (RegisteredFunctions reads as ' + (Get-Shown $registered) + ')')
        }
    } catch {
        [void]$loaded.Add("the registered XLL functions cannot be read: $($_.Exception.Message)")
    }
    return @{ loaded = $loaded; states = $states; connected = $connectedAddIns; installed = $installedAddIns }
}

# Dot-sourced, the script ends here, with its functions defined and nothing started.
if ($MyInvocation.InvocationName -eq '.') { return }
if ([string]::IsNullOrEmpty($Request) -or [string]::IsNullOrEmpty($Result)) {
    [Console]::Error.WriteLine('capture-worker.ps1 needs -Request <request.json> and -Result <result.json>')
    exit 2
}

$Output = [ordered]@{
    format = 'dnacalc-capture-worker-result'
    version = 3
    excel_pid = $null
    fatal = $null
    environment = $null
    sheets = @()
    formulas = @()
    cells = (New-Object System.Collections.ArrayList)
    saved = (New-Object System.Collections.ArrayList)
    saved_formulas = @()
    spill_members = (New-Object System.Collections.ArrayList)
    reopened = (New-Object System.Collections.ArrayList)
    failures = $Failures
}
$excel = $null
$workbook = $null
try {
    $req = [System.IO.File]::ReadAllText($Request, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
    # An .xlsb input is inventoried from its Excel-saved SpreadsheetML twin (bead dc-5sr): this
    # script reads SpreadsheetML parts only. Excel opens the input itself.
    $inventoryPath = $req.input
    if ($req.PSObject.Properties.Name -contains 'inventory' -and $req.inventory) { $inventoryPath = $req.inventory }
    $package = Read-Package $inventoryPath
    $Output.sheets = $package.sheets
    $Output.formulas = $package.formulas
    $inventory = @{}
    foreach ($entry in $package.formulas) { $inventory[(Get-Key $entry.sheet $entry.row $entry.column)] = $entry }

    $refusals = Get-PackageRefusals $package
    if ($refusals.Count -gt 0) {
        foreach ($refusal in $refusals) { Add-Failure 'isolation' $null "$refusal; Excel was not started, so the workbook was never opened" }
        throw ('refused before Excel was started: the package declares external content Excel could refresh or activate: ' + ($refusals -join '; '))
    }

    # The message filter needs this thread to be a single-threaded apartment, as Windows
    # PowerShell's is by default; registering fails otherwise, and the worker stops.
    $filterResult = [DnaCalcCapture.BusyRetryFilter]::Register()
    if ($filterResult -ne 0) { throw ('CoRegisterMessageFilter failed with HRESULT 0x' + $filterResult.ToString('X8')) }
    $excel = New-Object -ComObject Excel.Application
    $excelPid = Get-ExcelPid $excel
    $Output.excel_pid = $excelPid
    Send-Line "excel-pid $excelPid"

    # The session settings (see the header): each belongs to this Excel instance alone.
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $excel.ScreenUpdating = $false
    $excel.EnableEvents = $false
    $excel.AutomationSecurity = 3
    $excel.Interactive = $false
    # Each read back through Get-ComProperty, so a read that fails is not taken for the value set.
    $expected = [ordered]@{ DisplayAlerts = $false; EnableEvents = $false; Interactive = $false; AutomationSecurity = 3 }
    $notIsolated = New-Object System.Collections.ArrayList
    foreach ($name in $expected.Keys) {
        $want = $expected[$name]
        try {
            $value = Get-ComProperty $excel $name
        } catch {
            [void]$notIsolated.Add("$name cannot be read back after it was set to ${want}: $($_.Exception.Message)")
            continue
        }
        if ($want -is [bool]) {
            $same = ($value -is [bool]) -and ($value -eq $want)
        } else {
            $same = ($value -is [int] -or $value -is [long] -or $value -is [int16]) -and ([long]$value -eq [long]$want)
        }
        if (-not $same) { [void]$notIsolated.Add("$name reads back as " + (Get-Shown $value) + " after it was set to $want") }
    }
    # Add-ins are read, never switched off: one that is open in this instance, one whose state
    # cannot be read, and registered XLL functions refuse the capture before the workbook is
    # opened; connected COM add-ins and installed add-ins are recorded by name (Get-AddInStates).
    $addIns = Get-AddInStates $excel
    foreach ($entry in $addIns.loaded) { [void]$notIsolated.Add($entry) }
    if ($notIsolated.Count -gt 0) {
        foreach ($entry in $notIsolated) { Add-Failure 'isolation' $null "$entry; the workbook was not opened" }
        throw ('refused before the workbook was opened: this Excel instance is not isolated (a capture needs an Excel instance with no add-in open in it, and nothing was switched off): ' + ($notIsolated -join '; '))
    }

    $modes = @{ automatic = -4105; manual = -4135; semiautomatic = 2 }
    # A manual capture opens the workbook without calculating it: Excel takes the calculation mode
    # from the first workbook it opens, so a blank workbook is added first, set to manual, and
    # closed once the input is open. The full rebuild below is then the only calculation from the
    # package's stored values. An automatic Excel calculates a workbook that iterates when it
    # opens it, and again on each assignment of Calculation, Iteration, MaxIterations or
    # MaxChange, even of the value it holds (Excel 16.0.20430, 2026-10-25: A1 =B1+1 and B1 =A1+1
    # stored as 77 and 55 read 254 and 255 after the open, then 200 more after each assignment;
    # opened under manual calculation they read 77 and 55 until the rebuild, then 254 and 255).
    # Each setting is therefore assigned only when Excel does not already hold it.
    if ($req.calculation.mode -eq 'manual') {
        $blank = $excel.Workbooks.Add()
        $excel.Calculation = $modes['manual']
        $workbook = Open-Workbook $excel $req.input
        $blank.Close($false)
        $blank = $null
    } else {
        $workbook = Open-Workbook $excel $req.input
    }
    $wantedMode = $modes[$req.calculation.mode]
    $wantedIteration = [bool]$req.calculation.iteration
    $wantedCount = [int]$req.calculation.max_iterations
    $wantedChange = [BitConverter]::Int64BitsToDouble([Convert]::ToInt64($req.calculation.max_change, 16))
    if ((Get-ComProperty $excel 'Calculation') -ne $wantedMode) { $excel.Calculation = $wantedMode }
    if ((Get-ComProperty $excel 'Iteration') -ne $wantedIteration) { $excel.Iteration = $wantedIteration }
    if ((Get-ComProperty $excel 'MaxIterations') -ne $wantedCount) { $excel.MaxIterations = $wantedCount }
    if ((Get-ComProperty $excel 'MaxChange') -ne $wantedChange) { $excel.MaxChange = $wantedChange }
    $excel.CalculateFullRebuild()
    # XlCalculationState: xlDone 0, xlCalculating 1, xlPending 2. A read that fails throws, and a
    # value that is not an integer is refused, so neither reads as done (or not done); the
    # adapter's timeout bounds the wait. With iteration on, Excel 16.0.20430 (2026-10-25) leaves
    # the state at xlPending after the rebuild whenever the workbook holds a circular reference,
    # converged or not, and calculates nothing more on its own: the wait then ends once Excel is
    # not calculating, and the state it ends in is recorded (setting calculation_state).
    while ($true) {
        $state = Get-ComProperty $excel 'CalculationState'
        if ($state -isnot [int]) { throw ('Application.CalculationState reads as ' + (Get-Shown $state) + ', not an XlCalculationState') }
        if ($state -eq 0) { break }
        if ($state -eq 2 -and $wantedIteration) { break }
        Start-Sleep -Milliseconds 50
    }
    $calculationState = @{ 0 = 'done'; 2 = 'pending' }[$state]

    # The environment and settings, read back from Excel after the policy was set.
    $settings = New-Object System.Collections.ArrayList
    # A setting that does not come from Excel's object model (a .NET or WMI read, or a list built
    # above); one that is nothing is kept as empty and recorded as an environment failure.
    function Add-Setting([string]$Name, $Value) {
        if ($null -eq $Value) { Add-Failure 'environment' $null "the setting $Name reads as nothing" }
        [void]$settings.Add(@($Name, [string]$Value))
    }
    # The text a setting of $Kind is recorded as, or $null when $Value is not of that kind's type:
    # a flag as true or false, an integer in decimal, a number as its binary64 bits in hex, a text
    # as itself, and a scalar (an Application.International value) as its text or integer. Read
    # through Get-ComProperty (InvokeMember), Excel 16.0.20430 gave Application.Build,
    # MaxIterations and International(1), (2) and (32) as doubles (CalculationVersion as an
    # integer), so an integer is also a double that holds a whole number (within 2^53); any other
    # double is not one.
    function Format-Setting($Value, [string]$Kind) {
        $integer = ($Value -is [int]) -or ($Value -is [long]) -or ($Value -is [int16]) -or ($Value -is [byte])
        $text = $null
        if ($integer) {
            $text = [string]$Value
        } elseif ($Value -is [double] -and [Math]::Abs($Value) -le 9007199254740992 -and $Value -eq [Math]::Truncate($Value)) {
            $text = ([int64]$Value).ToString([System.Globalization.CultureInfo]::InvariantCulture)
        }
        switch ($Kind) {
            'flag' { if ($Value -is [bool]) { return ([string]$Value).ToLowerInvariant() } }
            'integer' { if ($null -ne $text) { return $text } }
            'number' { if ($Value -is [double]) { return [BitConverter]::DoubleToInt64Bits($Value).ToString('x16') } }
            'text' { if ($Value -is [string]) { return $Value } }
            'scalar' {
                if ($Value -is [string]) { return $Value }
                if ($null -ne $text) { return $text }
            }
        }
        return $null
    }
    # A setting read from Excel's object model: through Get-ComProperty, where a failed read
    # throws, and then checked for its kind's type. Its recorded text, or $null, with an
    # environment failure, when the read fails or the value is of another type; nothing is cast
    # from a failed read, so a setting never records a default (a max_change of bits 0, a flag of
    # "", a calculation version of "") in place of what Excel did not give.
    function Get-SettingText([string]$Name, $Object, [string]$Property, [string]$Kind, [object[]]$Arguments = $null) {
        try {
            $value = Get-ComProperty $Object $Property $Arguments
            $text = Format-Setting $value $Kind
            if ($null -eq $text) { throw ('it reads as ' + (Get-Shown $value) + ", not of the kind $Kind") }
            return $text
        } catch {
            Add-Failure 'environment' $null "the setting $Name ($Property) cannot be read: $($_.Exception.Message)"
            return $null
        }
    }
    # Get-SettingText's text recorded as the setting $Name; a setting that cannot be read is
    # recorded as empty, which the conversion refuses for a calculation setting (convert.rs's
    # context).
    function Read-Setting([string]$Name, $Object, [string]$Property, [string]$Kind, [object[]]$Arguments = $null) {
        [void]$settings.Add(@($Name, [string](Get-SettingText $Name $Object $Property $Kind $Arguments)))
    }
    $excelPath = (Get-Process -Id $excelPid).Path
    $version = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($excelPath)
    $fileVersion = '{0}.{1}.{2}.{3}' -f $version.FileMajorPart, $version.FileMinorPart, $version.FileBuildPart, $version.FilePrivatePart
    $machine = Get-Machine $excelPath
    $architecture = $env:PROCESSOR_ARCHITEW6432
    if ([string]::IsNullOrEmpty($architecture)) { $architecture = $env:PROCESSOR_ARCHITECTURE }
    switch ($architecture) {
        'AMD64' { $cpuFamily = 'x86-64' }
        'ARM64' { $cpuFamily = 'arm64' }
        'x86' { $cpuFamily = 'x86' }
        default { $cpuFamily = "$architecture".ToLowerInvariant() }
    }
    $channel = Get-Channel
    # The target's compatibility names Excel's calculation version, and the workbook's
    # compatibility version when that can be read. A calculation version that cannot be read
    # leaves the compatibility empty, which the conversion refuses (a target with an empty part),
    # rather than a target named without it. Both are recorded as settings, empty when unread.
    $calculationVersion = Get-SettingText 'calculation_version' $excel 'CalculationVersion' 'integer'
    $compatibility = ''
    if ($null -ne $calculationVersion) { $compatibility = "calc-$calculationVersion" }
    $workbookCompatibility = Get-SettingText 'workbook_compatibility_version' $workbook 'CompatibilityVersion' 'integer'
    if ($null -ne $workbookCompatibility -and $compatibility -ne '') { $compatibility = "compat-$workbookCompatibility/$compatibility" }
    [void]$settings.Add(@('workbook_compatibility_version', [string]$workbookCompatibility))
    Read-Setting 'excel_version' $excel 'Version' 'text'
    Read-Setting 'excel_build_number' $excel 'Build' 'integer'
    Add-Setting 'excel_machine' $machine
    [void]$settings.Add(@('calculation_version', [string]$calculationVersion))
    Read-Setting 'workbook_calculation_version' $workbook 'CalculationVersion' 'integer'
    Add-Setting 'channel_source' $channel[1]
    Read-Setting 'operating_system' $excel 'OperatingSystem' 'text'
    Add-Setting 'os_version' ([System.Environment]::OSVersion.VersionString)
    Add-Setting 'cpu_architecture' $architecture
    try {
        Add-Setting 'cpu' ((Get-CimInstance -ClassName Win32_Processor | Select-Object -First 1).Name)
    } catch {
        Add-Failure 'environment' $null "the CPU could not be read: $($_.Exception.Message)"
    }
    Add-Setting 'processor_count' ([System.Environment]::ProcessorCount)
    Add-Setting 'user_culture' ([System.Globalization.CultureInfo]::CurrentCulture.Name)
    # msoLanguageIDUI 2 and msoLanguageIDInstall 1.
    try {
        $languageSettings = Get-ComProperty $excel 'LanguageSettings'
        if ($null -eq $languageSettings) { throw 'it reads as nothing' }
    } catch {
        Add-Failure 'environment' $null "Application.LanguageSettings cannot be read: $($_.Exception.Message)"
        $languageSettings = $null
    }
    foreach ($language in @(@('ui_language_id', 2), @('install_language_id', 1))) {
        if ($null -eq $languageSettings) {
            [void]$settings.Add(@($language[0], ''))
        } else {
            Read-Setting $language[0] $languageSettings 'LanguageID' 'integer' @($language[1])
        }
    }
    # Application.International indices of XlApplicationInternational: xlCountryCode 1,
    # xlCountrySetting 2, xlDecimalSeparator 3, xlThousandsSeparator 4, xlListSeparator 5,
    # xlDateSeparator 17, xlDateOrder 32.
    $international = [ordered]@{ country_code = 1; country_setting = 2; decimal_separator = 3; thousands_separator = 4; list_separator = 5; date_separator = 17; date_order = 32 }
    foreach ($name in $international.Keys) { Read-Setting $name $excel 'International' 'scalar' @($international[$name]) }
    Read-Setting 'use_system_separators' $excel 'UseSystemSeparators' 'flag'
    Read-Setting 'date1904' $workbook 'Date1904' 'flag'
    # XlCalculation: xlCalculationAutomatic -4105, xlCalculationManual -4135 and
    # xlCalculationSemiautomatic 2; any other value is an environment failure, recorded as empty.
    $modeNames = @{ '-4105' = 'automatic'; '-4135' = 'manual'; '2' = 'semiautomatic' }
    $mode = Get-SettingText 'calculation' $excel 'Calculation' 'integer'
    if ($null -ne $mode -and -not $modeNames.ContainsKey($mode)) {
        Add-Failure 'environment' $null "the setting calculation (Calculation) reads as $mode, which is no XlCalculation mode"
        $mode = $null
    }
    if ($null -ne $mode) { $mode = $modeNames[$mode] }
    [void]$settings.Add(@('calculation', [string]$mode))
    Read-Setting 'iteration' $excel 'Iteration' 'flag'
    Add-Setting 'calculation_state' $calculationState
    Read-Setting 'max_iterations' $excel 'MaxIterations' 'integer'
    Read-Setting 'max_change' $excel 'MaxChange' 'number'
    Read-Setting 'precision_as_displayed' $workbook 'PrecisionAsDisplayed' 'flag'
    Read-Setting 'automation_security' $excel 'AutomationSecurity' 'integer'
    Read-Setting 'enable_events' $excel 'EnableEvents' 'flag'
    Read-Setting 'interactive' $excel 'Interactive' 'flag'
    Read-Setting 'display_alerts' $excel 'DisplayAlerts' 'flag'
    # The owner's options as this instance reads them; the worker never sets either.
    Read-Setting 'ask_to_update_links' $excel 'AskToUpdateLinks' 'flag'
    Read-Setting 'ignore_remote_requests' $excel 'IgnoreRemoteRequests' 'flag'
    Add-Setting 'add_ins' ($addIns.states -join ';')
    Add-Setting 'com_add_ins_connected' ($addIns.connected -join ';')
    Add-Setting 'add_ins_installed' ($addIns.installed -join ';')
    Read-Setting 'has_vb_project' $workbook 'HasVBProject' 'flag'
    # Whether Excel opened the input read-only, as Open-Workbook's third argument asks.
    Read-Setting 'opened_read_only' $workbook 'ReadOnly' 'flag'

    # External content, recorded and never refreshed: links to other workbooks (xlExcelLinks 1),
    # OLE and DDE links (xlOLELinks 2, an isolation failure), and data connections with their
    # refresh-on-open setting (one that refreshes on open, or whose setting cannot be read, is an
    # isolation failure).
    $links = $workbook.LinkSources(1)
    $linkNames = @()
    if ($null -ne $links) { foreach ($link in $links) { $linkNames += (Get-ListField (Get-LeafName ([string]$link))) } }
    Add-Setting 'external_links' ($linkNames -join ';')
    $oleLinks = $workbook.LinkSources(2)
    $oleNames = @()
    if ($null -ne $oleLinks) { foreach ($link in $oleLinks) { $oleNames += [string]$link } }
    Add-Setting 'ole_dde_links' (($oleNames | ForEach-Object { Get-ListField $_ }) -join ';')
    if ($oleNames.Count -gt 0) {
        Add-Failure 'isolation' $null ('the workbook holds OLE or DDE links, which this capture never updates: ' + ($oleNames -join '; '))
    }
    $packageConnections = @{}
    foreach ($declared in $package.connections) { $packageConnections[$declared.name] = $declared }
    $connectionSettings = New-Object System.Collections.ArrayList
    try {
        $workbookConnections = Get-ComProperty $workbook 'Connections'
        if ($null -eq $workbookConnections) { throw 'Workbook.Connections reads as nothing' }
    } catch {
        Add-Failure 'isolation' $null "the data connections of the workbook cannot be listed: $($_.Exception.Message)"
        $workbookConnections = @()
    }
    $index = 0
    foreach ($connection in $workbookConnections) {
        $index += 1
        try {
            $name = Get-ComProperty $connection 'Name'
            if ($name -isnot [string]) { throw ('its Name reads as ' + (Get-Shown $name)) }
            $type = Get-ComProperty $connection 'Type'
            if ($type -isnot [int]) { throw ('its Type reads as ' + (Get-Shown $type)) }
            # xlConnectionTypeOLEDB 1 and xlConnectionTypeODBC 2 carry RefreshOnFileOpen; for the
            # other types the package's refreshOnLoad of the connection of that name is read.
            switch ($type) {
                1 { $refresh = Get-Flag (Get-ComProperty $connection 'OLEDBConnection') 'RefreshOnFileOpen'; $source = 'excel' }
                2 { $refresh = Get-Flag (Get-ComProperty $connection 'ODBCConnection') 'RefreshOnFileOpen'; $source = 'excel' }
                default {
                    if (-not $packageConnections.ContainsKey($name)) {
                        throw "its type $type has no refresh-on-open setting in Excel's object model, and the package declares no connection named '$name'"
                    }
                    $refresh = [bool]$packageConnections[$name].refresh_on_load
                    $source = 'package'
                }
            }
            [void]$connectionSettings.Add((Get-ListField $name) + "|type $type|refresh_on_open " + ([string]$refresh).ToLowerInvariant() + " ($source)")
            if ($refresh) { Add-Failure 'isolation' $null "data connection '$name' refreshes when the workbook opens; this capture did not refresh it" }
        } catch {
            Add-Failure 'isolation' $null "the refresh-on-open setting of data connection $index cannot be read: $($_.Exception.Message)"
        }
    }
    Add-Setting 'connections' ($connectionSettings -join ';')

    foreach ($entry in $package.formulas) {
        $cell = Read-Cell $workbook $entry
        try {
            $has = Get-ComProperty $cell 'HasFormula'
            if ($has -isnot [bool]) {
                Add-Failure 'inventory' $entry ('the package holds a formula here, and Excel reads HasFormula as ' + (Get-Shown $has))
            } elseif (-not $has) {
                Add-Failure 'inventory' $entry 'the package holds a formula here, Excel reports none'
            }
        } catch {
            Add-Failure 'inventory' $entry "the package holds a formula here, and Range.HasFormula cannot be read: $($_.Exception.Message)"
        }
        try {
            $formula = Get-ComProperty $cell 'Formula2'
            if ($formula -isnot [string]) { throw ('it reads as ' + (Get-Shown $formula)) }
        } catch {
            throw "Range.Formula2 cannot be read at $($entry.sheet)!R$($entry.row)C$($entry.column) ($($_.Exception.Message)); this capture reads formula text through Formula2 only, so it is refused"
        }
        [void]$Output.cells.Add([ordered]@{ sheet = $entry.sheet; row = $entry.row; column = $entry.column; formula = (Get-Units $formula); value = (Read-Value $cell); spill = (Read-Spill $cell) })
    }
    # The inventory cells that spill, by sheet, each covering the rows and columns of its
    # SpillingToRange from itself.
    $spilling = @{}
    foreach ($read in $Output.cells) {
        if ($null -eq $read.spill -or -not $read.spill.Contains('rows')) { continue }
        if (-not $spilling.ContainsKey($read.sheet)) { $spilling[$read.sheet] = New-Object System.Collections.ArrayList }
        [void]$spilling[$read.sheet].Add($read)
    }
    # Excel's own formula cells, cross-checked against the package's inventory. Range.HasFormula is
    # False when no cell of the range holds a formula, True when all do and null when some do; only
    # Excel's own False skips the listing, and any error of the listing is an inventory failure. A
    # listed cell the package holds no formula for is a spill member when it lies in the spill range
    # of an inventory cell read above (Excel may list the cells a dynamic-array formula spills into
    # among its formula cells): it is recorded in spill_members, never in the inventory, and the
    # conversion checks it against the spill it records. Any other is an inventory failure. Each
    # cell of a legacy array formula's range is an inventory cell (Read-Package lists the members
    # beside the anchor), so Excel listing it is no failure.
    try {
        $worksheets = Get-ComProperty $workbook 'Worksheets'
        if ($null -eq $worksheets) { throw 'Workbook.Worksheets reads as nothing' }
    } catch {
        Add-Failure 'inventory' $null "the worksheets cannot be listed to cross-check the inventory: $($_.Exception.Message)"
        $worksheets = @()
    }
    foreach ($worksheet in $worksheets) {
        try {
            $sheetName = Get-ComProperty $worksheet 'Name'
            if ($sheetName -isnot [string]) { throw ('it reads as ' + (Get-Shown $sheetName)) }
        } catch {
            Add-Failure 'inventory' $null "a worksheet's name cannot be read, so its formula cells are not cross-checked: $($_.Exception.Message)"
            continue
        }
        try {
            $usedRange = Get-ComProperty $worksheet 'UsedRange'
            if ($null -eq $usedRange) { throw 'UsedRange reads as nothing' }
            $hasFormula = Get-ComProperty $usedRange 'HasFormula'
        } catch {
            Add-Failure 'inventory' $null "the formula cells of sheet $sheetName cannot be listed: UsedRange.HasFormula failed: $($_.Exception.Message)"
            continue
        }
        if ($hasFormula -is [bool] -and -not $hasFormula) { continue }
        try {
            $found = Get-ComProperty ($usedRange.SpecialCells(-4123)) 'Cells'
            if ($null -eq $found) { throw 'its Cells read as nothing' }
        } catch {
            Add-Failure 'inventory' $null "the formula cells of sheet $sheetName cannot be listed: SpecialCells(xlCellTypeFormulas) failed: $($_.Exception.Message)"
            continue
        }
        foreach ($cell in $found) {
            try {
                $row = Get-ComProperty $cell 'Row'
                $column = Get-ComProperty $cell 'Column'
                if (-not ($row -is [int] -and $column -is [int])) { throw ('its Row and Column read as ' + (Get-Shown $row) + ' and ' + (Get-Shown $column)) }
            } catch {
                Add-Failure 'inventory' $null "a formula cell Excel lists on sheet $sheetName cannot be placed: $($_.Exception.Message)"
                continue
            }
            if (-not $inventory.ContainsKey((Get-Key $sheetName $row $column))) {
                $member = $false
                if ($spilling.ContainsKey($sheetName)) {
                    foreach ($read in $spilling[$sheetName]) {
                        if ($row -ge $read.row -and $row -lt $read.row + $read.spill.rows -and $column -ge $read.column -and $column -lt $read.column + $read.spill.columns) { $member = $true }
                    }
                }
                if ($member) {
                    [void]$Output.spill_members.Add([ordered]@{ sheet = $sheetName; row = $row; column = $column })
                } else {
                    Add-Failure 'inventory' ([ordered]@{ sheet = $sheetName; row = $row; column = $column }) 'Excel reports a formula here that the package does not hold'
                }
            }
        }
    }
    $Output.environment = [ordered]@{
        excel_build = $fileVersion
        channel = $channel[0]
        compatibility = $compatibility
        numeric_environment = "excel-$machine/cpu-$cpuFamily"
        settings = $settings
    }

    $format = 51
    if ($req.output.ToLowerInvariant().EndsWith('.xlsm')) { $format = 52 }
    $workbook.SaveAs($req.output, $format)
    $workbook.Close($false)
    $workbook = $null

    $saved = Read-Package $req.output
    $Output.saved_formulas = $saved.formulas
    foreach ($entry in $package.formulas) {
        $cache = $saved.caches[(Get-Key $entry.sheet $entry.row $entry.column)]
        if ($null -eq $cache) {
            Add-Failure 'save' $entry 'the saved package holds no cached value for this formula cell'
        } else {
            [void]$Output.saved.Add([ordered]@{ sheet = $entry.sheet; row = $entry.row; column = $entry.column; t = $cache.t; v = (Get-Units $cache.v) })
        }
    }
    $savedFormulas = @{}
    foreach ($entry in $saved.formulas) { $savedFormulas[(Get-Key $entry.sheet $entry.row $entry.column)] = $entry }
    foreach ($entry in $package.formulas) {
        if (-not $savedFormulas.ContainsKey((Get-Key $entry.sheet $entry.row $entry.column))) { Add-Failure 'save' $entry 'the saved package holds no formula for this formula cell' }
    }
    foreach ($entry in $saved.formulas) {
        if (-not $inventory.ContainsKey((Get-Key $entry.sheet $entry.row $entry.column))) { Add-Failure 'save' $entry 'the saved package holds a formula here, which the input does not' }
    }

    $workbook = Open-Workbook $excel $req.output
    foreach ($entry in $package.formulas) {
        $cell = Read-Cell $workbook $entry
        [void]$Output.reopened.Add([ordered]@{ sheet = $entry.sheet; row = $entry.row; column = $entry.column; value = (Read-Value $cell); spill = (Read-Spill $cell) })
    }
    $workbook.Close($false)
    $workbook = $null
} catch {
    $Output.fatal = "$($_.Exception.Message) (capture-worker.ps1 line $($_.InvocationInfo.ScriptLineNumber))"
} finally {
    if ($null -ne $workbook) { try { $workbook.Close($false) } catch { } }
    if ($null -ne $excel) {
        try { $excel.Quit() } catch { }
        try { [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($excel) } catch { }
    }
    $workbook = $null
    $excel = $null
    [System.GC]::Collect()
    [System.GC]::WaitForPendingFinalizers()
    [System.GC]::Collect()
}
$json = ConvertTo-Json -InputObject $Output -Depth 12 -Compress
[System.IO.File]::WriteAllText($Result, $json, (New-Object System.Text.UTF8Encoding($false)))
Send-Line 'done'
if ($null -ne $Output.fatal) { exit 1 }
exit 0
