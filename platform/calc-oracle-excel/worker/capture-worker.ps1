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
#      range ref and cell metadata index (cm), the data connections with their refreshOnLoad flag,
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
#      every add-in (AddIns2: IsOpen and Installed; COMAddIns: Connect) and whether XLL functions
#      are registered. A setting that does not read back as set, an add-in that is open, installed
#      or connected, an add-in whose state cannot be read and registered XLL functions refuse the
#      capture as isolation failures, each named, and Excel quits without the workbook having been
#      opened. Nothing is switched off: a capture needs an Excel profile with no loaded add-ins;
#   5. opens the input read-only with UpdateLinks 0, sets the calculation policy of the request,
#      forces a full rebuild, and reads each inventory cell's formula (Formula2 only) and raw
#      Value2 (numbers as their binary64 bits), the environment and settings, the workbook's links
#      (an OLE or DDE link is an isolation failure) and its data connections (one that refreshes on
#      open, or whose setting cannot be read, is an isolation failure). Nothing is ever refreshed;
#   6. saves the workbook to the output path, closes it, reads the output package's caches for the
#      inventory cells, reopens the output read-only and reads each inventory cell's Value2 again;
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

# The worksheets of a package with their visibility, every cell that holds a formula or a value
# (formula type, shared group, ref and cell metadata index cm, and the cached value's type and
# text), the data connections of its connections part with their refreshOnLoad flag, the kind of
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
            if ($rel.type -ne ($RelPrefix + 'worksheet')) {
                # Macro sheets and dialog sheets; a macro sheet's relationship type is in
                # Microsoft's namespace, not the one $RelPrefix names.
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
            foreach ($c in $document.SelectNodes('/x:worksheet/x:sheetData/x:row/x:c', $sheetNames)) {
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
                    $entry = [ordered]@{ sheet = $name; row = $place.row; column = $place.column; type = $type; si = $null; ref = $null; cm = $null }
                    if ($f.HasAttribute('si')) { $entry.si = [int64]$f.GetAttribute('si') }
                    if ($f.HasAttribute('ref')) { $entry.ref = $f.GetAttribute('ref') }
                    if ($c.HasAttribute('cm')) { $entry.cm = $c.GetAttribute('cm') }
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
    # encrypted workbook fail to open instead of prompting.
    return $Application.Workbooks.Open($Path, 0, $true, $Missing, $NoPassword, $NoPassword, $true, $Missing, $Missing, $false, $false, $Missing, $false, $false, 0)
}

function Read-Cell($Workbook, $Entry) {
    return $Workbook.Worksheets.Item($Entry.sheet).Cells.Item($Entry.row, $Entry.column)
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

# The add-ins of an Excel instance, read and never changed: in loaded, each COM add-in that is
# connected, each add-in that is open or installed, each add-in or list whose state cannot be read,
# and registered XLL functions, every one named (by its full name, for the owner to find); in
# states, the state read of every add-in, named by its file name only, since the states go into
# the capture record and a full name can hold the user's profile path.
function Get-AddInStates($Application) {
    $loaded = New-Object System.Collections.ArrayList
    $states = New-Object System.Collections.ArrayList
    try {
        $comAddIns = Get-ComProperty $Application 'COMAddIns'
        if ($null -eq $comAddIns) { throw 'COMAddIns reads as nothing' }
        $index = 0
        foreach ($comAddIn in $comAddIns) {
            $index += 1
            $label = "COM add-in $index"
            try {
                $progId = Get-ComProperty $comAddIn 'ProgId'
                if ($progId -is [string] -and $progId -ne '') { $label = "COM add-in $progId" }
                $connected = Get-Flag $comAddIn 'Connect'
                [void]$states.Add("$label connect=" + ([string]$connected).ToLowerInvariant())
                if ($connected) { [void]$loaded.Add("$label is connected") }
            } catch {
                [void]$loaded.Add("the state of $label cannot be read: $($_.Exception.Message)")
            }
        }
    } catch {
        [void]$loaded.Add("the COM add-ins cannot be listed: $($_.Exception.Message)")
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
                    $label = "add-in $fullName"
                    $shown = 'add-in ' + [System.IO.Path]::GetFileName($fullName)
                }
                $open = Get-Flag $addIn 'IsOpen'
                $installed = Get-Flag $addIn 'Installed'
                [void]$states.Add("$shown open=" + ([string]$open).ToLowerInvariant() + ' installed=' + ([string]$installed).ToLowerInvariant())
                if ($open -or $installed) {
                    $how = @()
                    if ($open) { $how += 'open' }
                    if ($installed) { $how += 'installed' }
                    [void]$loaded.Add("$label is " + ($how -join ' and '))
                }
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
    return @{ loaded = $loaded; states = $states }
}

# Dot-sourced, the script ends here, with its functions defined and nothing started.
if ($MyInvocation.InvocationName -eq '.') { return }
if ([string]::IsNullOrEmpty($Request) -or [string]::IsNullOrEmpty($Result)) {
    [Console]::Error.WriteLine('capture-worker.ps1 needs -Request <request.json> and -Result <result.json>')
    exit 2
}

$Output = [ordered]@{
    format = 'dnacalc-capture-worker-result'
    version = 1
    excel_pid = $null
    fatal = $null
    environment = $null
    sheets = @()
    formulas = @()
    cells = (New-Object System.Collections.ArrayList)
    saved = (New-Object System.Collections.ArrayList)
    reopened = (New-Object System.Collections.ArrayList)
    failures = $Failures
}
$excel = $null
$workbook = $null
try {
    $req = [System.IO.File]::ReadAllText($Request, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
    $package = Read-Package $req.input
    $Output.sheets = $package.sheets
    $Output.formulas = $package.formulas
    $inventory = @{}
    foreach ($entry in $package.formulas) { $inventory[(Get-Key $entry.sheet $entry.row $entry.column)] = $true }

    $refusals = Get-PackageRefusals $package
    if ($refusals.Count -gt 0) {
        foreach ($refusal in $refusals) { Add-Failure 'isolation' $null "$refusal; Excel was not started, so the workbook was never opened" }
        throw ('refused before Excel was started: the package declares external content Excel could refresh or activate: ' + ($refusals -join '; '))
    }

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
    # Add-ins are read, never switched off: one that is loaded, or whose state cannot be read,
    # refuses the capture before the workbook is opened.
    $addIns = Get-AddInStates $excel
    foreach ($entry in $addIns.loaded) { [void]$notIsolated.Add($entry) }
    if ($notIsolated.Count -gt 0) {
        foreach ($entry in $notIsolated) { Add-Failure 'isolation' $null "$entry; the workbook was not opened" }
        throw ('refused before the workbook was opened: this Excel instance is not isolated (a capture needs an Excel profile with no loaded add-ins, and nothing was switched off): ' + ($notIsolated -join '; '))
    }

    $workbook = Open-Workbook $excel $req.input
    $modes = @{ automatic = -4105; manual = -4135; semiautomatic = 2 }
    $excel.Calculation = $modes[$req.calculation.mode]
    $excel.Iteration = [bool]$req.calculation.iteration
    $excel.MaxIterations = [int]$req.calculation.max_iterations
    $excel.MaxChange = [BitConverter]::Int64BitsToDouble([Convert]::ToInt64($req.calculation.max_change, 16))
    $excel.CalculateFullRebuild()
    # XlCalculationState: xlDone 0, xlCalculating 1, xlPending 2. A read that fails throws, and a
    # value that is not an integer is refused, so neither reads as done (or not done); the
    # adapter's timeout bounds the wait.
    while ($true) {
        $state = Get-ComProperty $excel 'CalculationState'
        if ($state -isnot [int]) { throw ('Application.CalculationState reads as ' + (Get-Shown $state) + ', not an XlCalculationState') }
        if ($state -eq 0) { break }
        Start-Sleep -Milliseconds 50
    }

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
    # as itself, and a scalar (an Application.International value) as its text or integer.
    function Format-Setting($Value, [string]$Kind) {
        $integer = ($Value -is [int]) -or ($Value -is [long]) -or ($Value -is [int16]) -or ($Value -is [byte])
        switch ($Kind) {
            'flag' { if ($Value -is [bool]) { return ([string]$Value).ToLowerInvariant() } }
            'integer' { if ($integer) { return [string]$Value } }
            'number' { if ($Value -is [double]) { return [BitConverter]::DoubleToInt64Bits($Value).ToString('x16') } }
            'text' { if ($Value -is [string]) { return $Value } }
            'scalar' { if ($integer -or $Value -is [string]) { return [string]$Value } }
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
    Read-Setting 'has_vb_project' $workbook 'HasVBProject' 'flag'

    # External content, recorded and never refreshed: links to other workbooks (xlExcelLinks 1),
    # OLE and DDE links (xlOLELinks 2, an isolation failure), and data connections with their
    # refresh-on-open setting (one that refreshes on open, or whose setting cannot be read, is an
    # isolation failure).
    $links = $workbook.LinkSources(1)
    $linkNames = @()
    if ($null -ne $links) { foreach ($link in $links) { $linkNames += [System.IO.Path]::GetFileName([string]$link) } }
    Add-Setting 'external_links' ($linkNames -join ';')
    $oleLinks = $workbook.LinkSources(2)
    $oleNames = @()
    if ($null -ne $oleLinks) { foreach ($link in $oleLinks) { $oleNames += [string]$link } }
    Add-Setting 'ole_dde_links' ($oleNames -join ';')
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
            [void]$connectionSettings.Add("$name|type $type|refresh_on_open " + ([string]$refresh).ToLowerInvariant() + " ($source)")
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
        [void]$Output.cells.Add([ordered]@{ sheet = $entry.sheet; row = $entry.row; column = $entry.column; formula = (Get-Units $formula); value = (Read-Value $cell) })
    }
    # Excel's own formula cells, cross-checked against the package's inventory. Range.HasFormula is
    # False when no cell of the range holds a formula, True when all do and null when some do; only
    # Excel's own False skips the listing, and any error of the listing is an inventory failure.
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
                Add-Failure 'inventory' ([ordered]@{ sheet = $sheetName; row = $row; column = $column }) 'Excel reports a formula here that the package does not hold'
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
    foreach ($entry in $package.formulas) {
        $cache = $saved.caches[(Get-Key $entry.sheet $entry.row $entry.column)]
        if ($null -eq $cache) {
            Add-Failure 'save' $entry 'the saved package holds no cached value for this formula cell'
        } else {
            [void]$Output.saved.Add([ordered]@{ sheet = $entry.sheet; row = $entry.row; column = $entry.column; t = $cache.t; v = (Get-Units $cache.v) })
        }
    }
    $savedFormulas = @{}
    foreach ($entry in $saved.formulas) { $savedFormulas[(Get-Key $entry.sheet $entry.row $entry.column)] = $true }
    foreach ($key in $inventory.Keys) {
        if (-not $savedFormulas.ContainsKey($key)) { Add-Failure 'save' $null "the saved package holds no formula at $key" }
    }
    foreach ($key in $savedFormulas.Keys) {
        if (-not $inventory.ContainsKey($key)) { Add-Failure 'save' $null "the saved package holds a formula at $key that the input did not" }
    }

    $workbook = Open-Workbook $excel $req.output
    foreach ($entry in $package.formulas) {
        $cell = Read-Cell $workbook $entry
        [void]$Output.reopened.Add([ordered]@{ sheet = $entry.sheet; row = $entry.row; column = $entry.column; value = (Read-Value $cell) })
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
