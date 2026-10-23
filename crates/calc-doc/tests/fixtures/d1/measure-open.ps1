# Measures what Excel does with a package's cached values and calculation chain when it opens it,
# for dc-ndi (D1): which caches a save writes, when it sets fullCalcOnLoad, and whether it writes
# or removes calcChain.xml. `cargo xtask capture` cannot measure this: it recalculates
# (CalculateFullRebuild) before it reads a value, by design.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File measure-open.ps1 -Out <open.json>
#
# The script starts its own Excel, builds a base workbook (Sheet1: A1 = 1, A2 = =A1+1,
# A3 = =A2*10, A4 = =SUM(A1:A3), B1 = ="t"&A1, B2 = =A1>0, B3 = =1/0), saves it as .xlsx, and
# writes variants of the saved package by editing its parts:
#   wrong-cache      A2's <v> is 99;
#   no-value         A2 has no <v>;
#   wrong-full       A2's <v> is 99 and calcPr has fullCalcOnLoad="1";
#   no-chain         calcChain.xml, its relationship and its content type removed;
#   chain-missing    A5 = =A4+1 (cached 25) added, calcChain.xml unchanged (no A5 entry);
#   chain-extra      calcChain.xml lists A1, which holds a constant;
#   no-chain-new     A5 = =A4+1 (cached 25) added and calcChain.xml removed;
#   old-calc-id      A2's <v> is 99 and calcPr's calcId is 124519 (an older Excel);
# and, for dc-35p (D2), whose saves leave a formula without a value and remove the chain when the
# set of formula cells changes:
#   no-value-no-chain   A2 has no <v>, and calcChain.xml is removed;
#   no-values-no-chain  no formula has a <v> (C1's spill is emptied), and calcChain.xml is removed;
#   no-values-full      as no-values-no-chain, with fullCalcOnLoad="1" on calcPr;
#   array-no-value      C1 (=A1:A2*2, a dynamic array spilling to C1:C2) has no <v> and C2 is
#                       emptied, and calcChain.xml is removed.
# The base also holds C1 = =A1:A2*2, entered as a dynamic array.
# Each variant, and the base, is opened in a fresh Excel (automatic calculation, alerts off, a
# normal load) and, without any calculation call, the script records whether it opened and the
# Value2 of A2, A3, A4, A5, B1, B2 and B3. It stops only the Excel instances it started.

param([Parameter(Mandatory = $true)][string]$Out)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -Namespace D1O -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$work = Join-Path ([System.IO.Path]::GetTempPath()) ('d1-open-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
$base = Join-Path $work 'base.xlsx'

function Start-Excel {
    $excel = New-Object -ComObject Excel.Application
    [uint32]$excelPid = 0
    [void][D1O.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    return @($excel, $excelPid)
}
function Stop-Excel($excel, [uint32]$excelPid) {
    $excel.Quit()
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    if ($excelPid -ne 0) { Get-Process -Id $excelPid -ErrorAction SilentlyContinue | Stop-Process -Force }
}

$started = Start-Excel
$version = "$($started[0].Version) build $($started[0].Build)"
try {
    $book = $started[0].Workbooks.Add()
    $sheet = $book.Worksheets.Item(1)
    $sheet.Range('A1').Value2 = 1
    $sheet.Range('A2').Formula = '=A1+1'
    $sheet.Range('A3').Formula = '=A2*10'
    $sheet.Range('A4').Formula = '=SUM(A1:A3)'
    $sheet.Range('B1').Formula = '="t"&A1'
    $sheet.Range('B2').Formula = '=A1>0'
    $sheet.Range('B3').Formula = '=1/0'
    $sheet.Range('C1').Formula2 = '=A1:A2*2'
    $book.SaveAs($base, 51)
    $book.Close($false)
}
finally { Stop-Excel $started[0] $started[1] }

# The parts of a package, by name, as text (every part this script edits is XML).
function Read-Package([string]$Path) {
    $parts = [ordered]@{}
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Path)
    try {
        foreach ($entry in $zip.Entries) {
            $stream = $entry.Open(); $memory = New-Object System.IO.MemoryStream
            $stream.CopyTo($memory); $stream.Close()
            $parts[$entry.FullName] = $memory.ToArray()
        }
    } finally { $zip.Dispose() }
    return $parts
}
function Write-Package([string]$Path, $Parts) {
    if (Test-Path $Path) { Remove-Item $Path }
    $zip = [System.IO.Compression.ZipFile]::Open($Path, 'Create')
    try {
        foreach ($name in $Parts.Keys) {
            $entry = $zip.CreateEntry($name)
            $stream = $entry.Open(); $bytes = [byte[]]$Parts[$name]; $stream.Write($bytes, 0, $bytes.Length); $stream.Close()
        }
    } finally { $zip.Dispose() }
}
$utf8 = New-Object System.Text.UTF8Encoding($false)
function Get-Text($Parts, [string]$Name) { return $utf8.GetString([byte[]]$Parts[$Name]) }
function Set-Text($Parts, [string]$Name, [string]$Text) { $Parts[$Name] = $utf8.GetBytes($Text) }
function Copy-Parts($Parts) { $copy = [ordered]@{}; foreach ($k in $Parts.Keys) { $copy[$k] = $Parts[$k] }; return $copy }
function Remove-Chain($Parts) {
    $Parts.Remove('xl/calcChain.xml')
    $rels = Get-Text $Parts 'xl/_rels/workbook.xml.rels'
    $rels = [regex]::Replace($rels, '<Relationship [^>]*calcChain[^>]*/>', '')
    Set-Text $Parts 'xl/_rels/workbook.xml.rels' $rels
    $types = Get-Text $Parts '[Content_Types].xml'
    $types = [regex]::Replace($types, '<Override PartName="/xl/calcChain.xml"[^>]*/>', '')
    Set-Text $Parts '[Content_Types].xml' $types
}
function Add-A5($Parts) {
    $sheet = Get-Text $Parts 'xl/worksheets/sheet1.xml'
    $sheet = $sheet.Replace('</sheetData>', '<row r="5"><c r="A5"><f>A4+1</f><v>25</v></c></row></sheetData>')
    $sheet = [regex]::Replace($sheet, '<dimension ref="[^"]*"/>', '<dimension ref="A1:B5"/>')
    Set-Text $Parts 'xl/worksheets/sheet1.xml' $sheet
}
function Set-A2Cache($Parts, [string]$Value) {
    $sheet = Get-Text $Parts 'xl/worksheets/sheet1.xml'
    $sheet = [regex]::Replace($sheet, '(<c r="A2"[^>]*><f>A1\+1</f>)<v>[^<]*</v>', ('$1' + $Value))
    Set-Text $Parts 'xl/worksheets/sheet1.xml' $sheet
}

$parts = Read-Package $base
$variants = [ordered]@{ base = (Copy-Parts $parts) }
$v = Copy-Parts $parts; Set-A2Cache $v '<v>99</v>'; $variants['wrong-cache'] = $v
$v = Copy-Parts $parts; Set-A2Cache $v ''; $variants['no-value'] = $v
$v = Copy-Parts $parts; Set-A2Cache $v '<v>99</v>'
Set-Text $v 'xl/workbook.xml' ([regex]::Replace((Get-Text $v 'xl/workbook.xml'), '<calcPr ', '<calcPr fullCalcOnLoad="1" ')); $variants['wrong-full'] = $v
$v = Copy-Parts $parts; Remove-Chain $v; $variants['no-chain'] = $v
$v = Copy-Parts $parts; Add-A5 $v; $variants['chain-missing'] = $v
$v = Copy-Parts $parts
Set-Text $v 'xl/calcChain.xml' ((Get-Text $v 'xl/calcChain.xml').Replace('<calcChain xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">', '<calcChain xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><c r="A1" i="1"/>')); $variants['chain-extra'] = $v
$v = Copy-Parts $parts; Add-A5 $v; Remove-Chain $v; $variants['no-chain-new'] = $v
$v = Copy-Parts $parts; Set-A2Cache $v '<v>99</v>'
Set-Text $v 'xl/workbook.xml' ([regex]::Replace((Get-Text $v 'xl/workbook.xml'), 'calcId="[0-9]+"', 'calcId="124519"')); $variants['old-calc-id'] = $v
function Remove-Values($Parts) {
    $sheet = Get-Text $Parts 'xl/worksheets/sheet1.xml'
    $sheet = [regex]::Replace($sheet, '(</f>)<v>[^<]*</v>', '$1')
    $sheet = [regex]::Replace($sheet, '<c r="C2"[^>]*>(<v>[^<]*</v>)?</c>|<c r="C2"[^>]*/>', '')
    Set-Text $Parts 'xl/worksheets/sheet1.xml' $sheet
}
$v = Copy-Parts $parts; Set-A2Cache $v ''; Remove-Chain $v; $variants['no-value-no-chain'] = $v
$v = Copy-Parts $parts; Remove-Values $v; Remove-Chain $v; $variants['no-values-no-chain'] = $v
$v = Copy-Parts $parts; Remove-Values $v; Remove-Chain $v
Set-Text $v 'xl/workbook.xml' ([regex]::Replace((Get-Text $v 'xl/workbook.xml'), '<calcPr ', '<calcPr fullCalcOnLoad="1" ')); $variants['no-values-full'] = $v
$v = Copy-Parts $parts
$sheet = Get-Text $v 'xl/worksheets/sheet1.xml'
$sheet = [regex]::Replace($sheet, '(<c r="C1"[^>]*><f[^>]*>[^<]*</f>)<v>[^<]*</v>', '$1')
$sheet = [regex]::Replace($sheet, '<c r="C2"[^>]*>(<v>[^<]*</v>)?</c>|<c r="C2"[^>]*/>', '')
Set-Text $v 'xl/worksheets/sheet1.xml' $sheet; Remove-Chain $v; $variants['array-no-value'] = $v

$rows = @()
foreach ($name in $variants.Keys) {
    $path = Join-Path $work "$name.xlsx"
    Write-Package $path $variants[$name]
    $row = [ordered]@{ variant = $name; opened = $false; error = $null; values = [ordered]@{} }
    $started = Start-Excel
    try {
        $missing = [System.Reflection.Missing]::Value
        try {
            [object[]]$arguments = @([string]$path, 0, $true, $missing, $missing, $missing, $true, $missing, $missing, $false, $false, $missing, $false, $false, 0)
            $book = [System.__ComObject].InvokeMember('Open', [System.Reflection.BindingFlags]::InvokeMethod, $null, $started[0].Workbooks, $arguments)
            $row.opened = $true
            foreach ($address in 'A2', 'A3', 'A4', 'A5', 'B1', 'B2', 'B3', 'C1', 'C2') {
                $value = $book.Worksheets.Item(1).Range($address).Value2
                $row.values[$address] = [string]$value
            }
            $book.Close($false)
        }
        catch { $row.error = $_.Exception.Message }
    }
    finally { Stop-Excel $started[0] $started[1] }
    $rows += , $row
}
$chain = Get-Text $parts 'xl/calcChain.xml'
Remove-Item $work -Recurse -Force
$report = [ordered]@{ excel = $version; base_calc_chain = $chain; rows = $rows }
$report | ConvertTo-Json -Depth 5 | Set-Content -Path $Out -Encoding UTF8
$report | ConvertTo-Json -Depth 5
