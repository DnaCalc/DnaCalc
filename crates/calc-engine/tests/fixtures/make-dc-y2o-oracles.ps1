# The generator of the oracle workbooks of dc-y2o (circular references, iteration optional) under
# crates/calc-engine/tests/fixtures/:
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-dc-y2o-oracles.ps1 -OutDir <directory>
#
# writes <directory>/dc_y2o/oracle.xlsx and <directory>/dc_86f/oracle.xlsx. It starts an Excel
# instance of its own, enters every formula with Range.Formula, saves, and stops only that
# instance. It then rewrites the values the saved sheets store for the formula cells (each
# workbook's $Stored table below), with .NET's ZIP and XML classes and nothing else in the package
# changed, so that the stored values are not the ones Excel's own calculation left: a cell on a
# circular reference keeps what its cell held, and the rewritten values show which value that
# is. The committed workbooks are the oracles, and `cargo xtask capture` records what Excel
# calculates from them (the iteration-on workbook is captured under manual calculation, so its one
# full rebuild calculates from the stored values):
#
#   cargo xtask capture --fixture crates/calc-engine/tests/fixtures/dc_y2o/oracle.xlsx \
#       --out crates/calc-engine/tests/captures/dc_y2o/oracle.capture.json
#   cargo xtask capture --fixture crates/calc-engine/tests/fixtures/dc_86f/oracle.xlsx \
#       --out crates/calc-engine/tests/captures/dc_86f/oracle.capture.json
#
# dc_y2o/oracle.xlsx (iteration off, sheet Off): a two-cell cycle stored as 77 and 55, a reader of
# it and a reader of that reader stored with stale values, self-referencing formulas stored as a
# number, a text, an error and with no value, a guarded self-reference whose evaluation does not
# meet itself, a cycle only INDIRECT's text makes, a cycle through a range, and formulas outside
# every cycle stored with stale values.
#
# dc_86f/oracle.xlsx (iteration on, at most 1 iteration, every formula stored as 0): the order in
# which Excel evaluates the members of a cycle. Cycles laid out along a row, down a column and
# across rows and columns, entered in an order that is not cell order and with the saved calc
# chain reversed; and cycles across sheets named b, A and C (in that tab order, created in another
# order).

param(
    [Parameter(Mandatory = $true)][string]$OutDir
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
if (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path (Get-Location).Path $OutDir }
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -Namespace Y2o -Name W -MemberDefinition '[DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(System.IntPtr h, out int pid);'

# Excel is shared on this machine: a call it rejects while busy is retried.
function Retry([scriptblock]$Block) {
    for ($i = 0; $i -lt 120; $i++) {
        try { return & $Block } catch {
            if ($_.Exception.Message -match '0x80010001|0x8001010A|RPC_E_CALL_REJECTED|busy') { Start-Sleep -Milliseconds 500; continue }
            throw
        }
    }
    throw 'Excel rejected the call 120 times'
}

function Save-Workbook($Workbook, [string]$Path) {
    $dir = Split-Path $Path -Parent
    if (-not (Test-Path $dir)) { [void](New-Item -ItemType Directory -Path $dir) }
    if (Test-Path $Path) { Remove-Item $Path }
    $Workbook.SaveAs($Path, 51)
}

# The part name of each worksheet of the package, by sheet name.
function Get-SheetParts([System.IO.Compression.ZipArchive]$Zip) {
    $read = { param($name) $r = New-Object System.IO.StreamReader($Zip.GetEntry($name).Open()); try { [xml]$r.ReadToEnd() } finally { $r.Dispose() } }
    $workbook = & $read 'xl/workbook.xml'
    $rels = & $read 'xl/_rels/workbook.xml.rels'
    $parts = @{}
    foreach ($sheet in $workbook.workbook.sheets.sheet) {
        $id = $sheet.GetAttribute('id', 'http://schemas.openxmlformats.org/officeDocument/2006/relationships')
        $target = ($rels.Relationships.Relationship | Where-Object { $_.Id -eq $id }).Target
        $parts[$sheet.name] = 'xl/' + $target.TrimStart('/').Replace('xl/', '')
    }
    $parts
}

# Rewrites the stored value of formula cells: $Stored maps 'Sheet!A1' to a number, @('str', text),
# @('e', error text) or $null (the cell stores no value). With $ReverseChain the calc chain's
# entries are written in reverse order.
function Set-StoredValues([string]$Path, [hashtable]$Stored, [bool]$ReverseChain) {
    $zip = [System.IO.Compression.ZipFile]::Open($Path, 'Update')
    try {
        $parts = Get-SheetParts $zip
        $ns = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'
        foreach ($sheetName in @($Stored.Keys | ForEach-Object { $_.Split('!')[0] } | Select-Object -Unique)) {
            $entry = $zip.GetEntry($parts[$sheetName])
            $reader = New-Object System.IO.StreamReader($entry.Open())
            $doc = New-Object System.Xml.XmlDocument
            $doc.PreserveWhitespace = $true
            $doc.LoadXml($reader.ReadToEnd()); $reader.Dispose()
            $m = New-Object System.Xml.XmlNamespaceManager($doc.NameTable); $m.AddNamespace('s', $ns)
            foreach ($key in @($Stored.Keys | Where-Object { $_.StartsWith("$sheetName!") })) {
                $ref = $key.Split('!')[1]
                $cell = $doc.SelectSingleNode("//s:c[@r='$ref']", $m)
                if ($null -eq $cell -or $null -eq $cell.SelectSingleNode('s:f', $m)) { throw "$key holds no formula" }
                $old = $cell.SelectSingleNode('s:v', $m)
                if ($null -ne $old) { [void]$cell.RemoveChild($old) }
                $cell.RemoveAttribute('t')
                $value = $Stored[$key]
                if ($null -eq $value) { continue }
                $v = $doc.CreateElement('v', $ns)
                if ($value -is [array]) { $cell.SetAttribute('t', $value[0]); $v.InnerText = $value[1] }
                else { $v.InnerText = ([double]$value).ToString('R', [Globalization.CultureInfo]::InvariantCulture) }
                [void]$cell.AppendChild($v)
            }
            $entry.Delete()
            $new = $zip.CreateEntry($parts[$sheetName])
            $writer = New-Object System.IO.StreamWriter($new.Open(), (New-Object System.Text.UTF8Encoding($false)))
            $writer.Write($doc.OuterXml); $writer.Dispose()
        }
        if ($ReverseChain) {
            $entry = $zip.GetEntry('xl/calcChain.xml')
            $reader = New-Object System.IO.StreamReader($entry.Open())
            $doc = New-Object System.Xml.XmlDocument
            $doc.LoadXml($reader.ReadToEnd()); $reader.Dispose()
            $root = $doc.DocumentElement
            # Each entry names its sheet (i) explicitly once reordered; l (new sheet) is dropped.
            $sheetIndex = $null
            $entries = @()
            foreach ($c in @($root.ChildNodes)) {
                if ($c.HasAttribute('i')) { $sheetIndex = $c.GetAttribute('i') }
                $c.SetAttribute('i', $sheetIndex); $c.RemoveAttribute('l')
                $entries += $c; [void]$root.RemoveChild($c)
            }
            [array]::Reverse($entries)
            foreach ($c in $entries) { [void]$root.AppendChild($c) }
            $entry.Delete()
            $new = $zip.CreateEntry('xl/calcChain.xml')
            $writer = New-Object System.IO.StreamWriter($new.Open(), (New-Object System.Text.UTF8Encoding($false)))
            $writer.Write($doc.OuterXml); $writer.Dispose()
        }
    } finally { $zip.Dispose() }
}

$xl = New-Object -ComObject Excel.Application
$xlPid = 0
[void][Y2o.W]::GetWindowThreadProcessId([System.IntPtr]$xl.Hwnd, [ref]$xlPid)
try {
    Retry { $xl.Visible = $false; $xl.DisplayAlerts = $false; $xl.ScreenUpdating = $false } | Out-Null

    # --- dc_y2o/oracle.xlsx (iteration off) -----------------------------------------------------
    $wb = Retry { $xl.Workbooks.Add() }
    if ($xl.Iteration) { $xl.Iteration = $false }
    $s = $wb.Worksheets.Item(1); $s.Name = 'Off'
    $s.Range('C1').Value2 = 1
    $s.Range('L2').Value2 = 2
    $formulas = [ordered]@{
        'A1' = '=B1+1'; 'B1' = '=A1+1'; 'G1' = '=A1*10'; 'G2' = '=G1+C1'; 'G3' = '=B1+C1'
        'D1' = '=D1+1'; 'E1' = '=E1&"x"'; 'F1' = '=F1+1'; 'D2' = '=D2*2'; 'D3' = '=D2+1'
        'H1' = '=C1*100'; 'H2' = '=H1+1'; 'I1' = '=IF(C1>5,I1+1,7)'
        'J1' = '=INDIRECT("K1")+1'; 'K1' = '=INDIRECT("J1")+1'
        'L1' = '=SUM(L2:L3)'; 'L3' = '=L1/2'
    }
    foreach ($at in $formulas.Keys) { $s.Range($at).Formula = $formulas[$at] }
    $path = Join-Path $OutDir 'dc_y2o\oracle.xlsx'
    Save-Workbook $wb $path
    $wb.Close($false)
    Set-StoredValues $path @{
        'Off!A1' = 77; 'Off!B1' = 55; 'Off!G1' = 999; 'Off!G2' = 5; 'Off!G3' = 4
        'Off!D1' = 41; 'Off!E1' = @('str', 'abc'); 'Off!F1' = @('e', '#N/A'); 'Off!D2' = $null; 'Off!D3' = 8
        'Off!H1' = 3; 'Off!H2' = 4; 'Off!I1' = 1
        'Off!J1' = 5; 'Off!K1' = 6
        'Off!L1' = 12; 'Off!L3' = $null
    } $false

    # --- dc_86f/oracle.xlsx (iteration on, at most 1 iteration) --------------------------------
    $wb = Retry { $xl.Workbooks.Add() }
    while ($wb.Worksheets.Count -lt 4) { [void]$wb.Worksheets.Add([Type]::Missing, $wb.Worksheets.Item($wb.Worksheets.Count)) }
    $o = $wb.Worksheets.Item(1); $o.Name = 'Order'
    $c = $wb.Worksheets.Item(2); $c.Name = 'C'
    $a = $wb.Worksheets.Item(3); $a.Name = 'A'
    $b = $wb.Worksheets.Item(4); $b.Name = 'b'
    # Tab order Order, b, A, C; created as Order, C, A, b (sheetId 1, 2, 3, 4).
    $b.Move([Type]::Missing, $o); $a.Move([Type]::Missing, $b)
    if (-not $xl.Iteration) { $xl.Iteration = $true }
    if ($xl.MaxIterations -ne 1) { $xl.MaxIterations = 1 }
    if ($xl.MaxChange -ne 0.001) { $xl.MaxChange = 0.001 }
    # Each in the order entered.
    $order = @(
        @($o, 'B1', '=A1*2'), @($o, 'A1', '=B1+1'),
        @($o, 'F1', '=D1+1'), @($o, 'E1', '=F1*2'), @($o, 'D1', '=E1+1'),
        @($o, 'H2', '=I1*2'), @($o, 'I1', '=H1+1'), @($o, 'H1', '=H2+1'),
        @($o, 'A4', '=A3+1'), @($o, 'A3', '=A5*2'), @($o, 'A5', '=A4+1'),
        @($o, 'K1', '=K1+1'), @($o, 'L1', '=K1+L1'), @($o, 'M1', '=L1*10'),
        @($b, 'A1', '=C!A1+1'), @($c, 'A1', '=b!A1*2'),
        @($a, 'A2', '=b!A2+1'), @($b, 'A2', '=A!A2*2'),
        @($c, 'A3', '=A!A3+1'), @($a, 'A3', '=C!A3*2'),
        @($b, 'A4', '=Order!A10+1'), @($o, 'A10', '=b!A4*2')
    )
    foreach ($entry in $order) { $entry[0].Range($entry[1]).Formula = $entry[2] }
    $zeros = @{}
    foreach ($entry in $order) { $zeros[$entry[0].Name + '!' + $entry[1]] = 0 }
    $o.Activate()
    $path = Join-Path $OutDir 'dc_86f\oracle.xlsx'
    Save-Workbook $wb $path
    $wb.Close($false)
    Set-StoredValues $path $zeros $true
} finally {
    try { $xl.DisplayAlerts = $false; foreach ($open in @($xl.Workbooks)) { $open.Close($false) }; $xl.Quit() } catch {}
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($xl)
    Start-Sleep -Milliseconds 500
    if (Get-Process -Id $xlPid -ErrorAction SilentlyContinue) { Stop-Process -Id $xlPid -Force }
}
Write-Host "wrote $OutDir\dc_y2o\oracle.xlsx and $OutDir\dc_86f\oracle.xlsx"
