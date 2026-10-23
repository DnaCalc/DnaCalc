# Measures, for each worksheet function, which argument positions make Excel store an entered
# formula as a dynamic-array formula when a multi-cell range stands there, for dc-ndi (D1):
# calc_doc decides from this whether an authored formula is written as a dynamic array.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File measure-array-positions.ps1 -Prefixes <excel-prefixes.json> -Functions <functions.json> -Out <positions.json>
#
# <excel-prefixes.json> is measure-prefixes.ps1's output: the formula Excel accepted for each
# function. For each accepted function call FN(a1,...,an) (LET and LAMBDA left out: their
# arguments are names and bodies), the script enters into a fresh workbook, through
# Range.Formula2, the call with every argument the single cell Z1 (the base), and then, for each
# argument count from the accepted one up to the most the signature in <functions.json> names (at
# most six more), and then, with the most arguments, the call with argument i the range Z1:Z3 for
# each position i. It saves the workbook, and records for each entry whether the cell carries cm
# (Excel's dynamic-array cell metadata), or null when Excel refused the entry. Each row is [name,
# the accepted argument count n, [base flag for n, n+1, ...], [position flags at the most
# arguments]]. It stops only the Excel it started.

param(
    [Parameter(Mandatory = $true)][string]$Prefixes,
    [Parameter(Mandatory = $true)][string]$Functions,
    [Parameter(Mandatory = $true)][string]$Out
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -Namespace D1A -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$measured = (Get-Content -Raw $Prefixes | ConvertFrom-Json).rows
$maxima = @{}
foreach ($pair in (Get-Content -Raw $Functions | ConvertFrom-Json)) { $maxima[[string]$pair[0]] = [int]$pair[2] }
$work = Join-Path ([System.IO.Path]::GetTempPath()) ('d1-positions-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
$path = Join-Path $work 'positions.xlsx'

# The plan: each function's argument count and the cells its entries go to.
$plan = @()
$row = 0
foreach ($entry in $measured) {
    $name = [string]$entry[0]
    $accepted = $entry[1]
    if ($null -eq $accepted -or $name -eq 'LET' -or $name -eq 'LAMBDA') { continue }
    $inside = ([string]$accepted).Substring($name.Length + 2)
    $inside = $inside.Substring(0, $inside.Length - 1)
    $count = 0
    if ($inside -ne '') { $count = $inside.Split(',').Count }
    $most = [Math]::Min([Math]::Max($count, $maxima[$name]), $count + 6)
    $row++
    $plan += , @($name, $count, $most, $row)
}

$ok = @{}
$excel = New-Object -ComObject Excel.Application
[uint32]$excelPid = 0
[void][D1A.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
$version = "$($excel.Version) build $($excel.Build)"
try {
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $book = $excel.Workbooks.Add()
    $excel.Calculation = -4135
    $sheet = $book.Worksheets.Item(1)
    foreach ($item in $plan) {
        $name = $item[0]; $count = $item[1]; $most = $item[2]; $r = $item[3]
        # Row r * 8: the base call with each argument count from the accepted one to the most
        # the signature names, one per column B, D, ...; row r * 8 + 4: the call with the most
        # arguments and a range at each position in turn.
        for ($k = $count; $k -le $most; $k++) {
            $formula = '=' + $name + '(' + ((@('Z1') * $k) -join ',') + ')'
            $address = [char](66 + ($k - $count) * 2) + [string]($r * 8)
            try { $sheet.Range($address).Formula2 = $formula; $ok[$address] = $true } catch { $ok[$address] = $false }
        }
        for ($position = 0; $position -lt $most; $position++) {
            $arguments = @('Z1') * $most
            $arguments[$position] = 'Z1:Z3'
            $formula = '=' + $name + '(' + ($arguments -join ',') + ')'
            $address = [char](66 + $position * 2) + [string]($r * 8 + 4)
            try { $sheet.Range($address).Formula2 = $formula; $ok[$address] = $true } catch { $ok[$address] = $false }
        }
    }
    $book.SaveAs($path, 51)
    $book.Close($false)
}
finally {
    $excel.Quit()
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
    [GC]::Collect()
    [GC]::WaitForPendingFinalizers()
    if ($excelPid -ne 0) { Get-Process -Id $excelPid -ErrorAction SilentlyContinue | Stop-Process -Force }
}

$zip = [System.IO.Compression.ZipFile]::OpenRead($path)
try {
    $reader = New-Object System.IO.StreamReader($zip.GetEntry('xl/worksheets/sheet1.xml').Open())
    [xml]$xml = $reader.ReadToEnd()
    $reader.Close()
}
finally { $zip.Dispose() }
Remove-Item $work -Recurse -Force
$ns = New-Object System.Xml.XmlNamespaceManager($xml.NameTable)
$ns.AddNamespace('m', 'http://schemas.openxmlformats.org/spreadsheetml/2006/main')
$cells = @{}
foreach ($cell in $xml.SelectNodes('//m:c', $ns)) {
    if ($null -ne $cell.SelectSingleNode('m:f', $ns)) { $cells[$cell.GetAttribute('r')] = ($cell.GetAttribute('cm') -ne '') }
}

function Get-Flag([string]$Address) {
    if ($ok[$Address] -and $cells.ContainsKey($Address)) { return $cells[$Address] }
    return $null
}
$rows = @()
foreach ($item in $plan) {
    $name = $item[0]; $count = $item[1]; $most = $item[2]; $r = $item[3]
    $bases = @()
    for ($k = $count; $k -le $most; $k++) { $bases += , (Get-Flag ([char](66 + ($k - $count) * 2) + [string]($r * 8))) }
    $flags = @()
    for ($position = 0; $position -lt $most; $position++) { $flags += , (Get-Flag ([char](66 + $position * 2) + [string]($r * 8 + 4))) }
    $rows += , @($name, $count, $bases, $flags)
}
$report = [ordered]@{ excel = $version; rows = $rows }
$report | ConvertTo-Json -Depth 5 | Set-Content -Path $Out -Encoding UTF8
Write-Output "measured $($rows.Count) functions with Excel $version"
