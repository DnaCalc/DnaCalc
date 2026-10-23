# Measures how Excel stores formulas a user enters, for dc-ndi (D1): calc_doc writes authored
# formulas in that file form.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File measure-file-form.ps1 -Formulas <formulas.txt> -Out <file-form.json>
#
# <formulas.txt> holds one formula per line, as a user types it. The script starts its own Excel,
# fills Sheet1!A1:C3 with numbers and defines the names RangeName (=Sheet1!$A$1:$A$3) and
# ConstName (=5), enters each formula through Range.Formula2 in column F from row 10 (a spilling
# anchor E1, =SEQUENCE(3), gives A1#-style formulas a spill to read), saves the workbook as .xlsx
# and reads back each formula cell: its <f> text, the <f> attributes and the cell's cm. Each row is
# [entered, stored text, f type, f ref, cm], or [entered, null, ...] when Excel refused the entry.
# It stops only the Excel it started.

param(
    [Parameter(Mandatory = $true)][string]$Formulas,
    [Parameter(Mandatory = $true)][string]$Out
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -Namespace D1F -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$lines = @(Get-Content $Formulas | ForEach-Object { [string]$_ } | Where-Object { $_.Trim() -ne '' })
$work = Join-Path ([System.IO.Path]::GetTempPath()) ('d1-fileform-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
$path = Join-Path $work 'fileform.xlsx'
$accepted = @{}

$excel = New-Object -ComObject Excel.Application
[uint32]$excelPid = 0
[void][D1F.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
$version = "$($excel.Version) build $($excel.Build)"
try {
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $book = $excel.Workbooks.Add()
    $sheet = $book.Worksheets.Item(1)
    $sheet.Name = 'Sheet1'
    foreach ($row in 1..3) {
        foreach ($column in 'A', 'B', 'C') {
            $sheet.Range("$column$row").Value2 = [double]($row * 10 + [int][char]$column - 64)
        }
    }
    [void]$book.Names.Add('RangeName', '=Sheet1!$A$1:$A$3')
    [void]$book.Names.Add('ConstName', '=5')
    $sheet.Range('E1').Formula2 = '=SEQUENCE(3)'
    $row = 10
    foreach ($line in $lines) {
        try {
            $sheet.Range("F$row").Formula2 = $line
            $accepted[$row] = $true
        }
        catch { $accepted[$row] = $false }
        $row += 4
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
foreach ($cell in $xml.SelectNodes('//m:c', $ns)) { $cells[$cell.GetAttribute('r')] = $cell }

$rows = @()
$row = 10
foreach ($line in $lines) {
    $cell = $null
    if ($accepted[$row] -and $cells.ContainsKey("F$row")) { $cell = $cells["F$row"] }
    $f = $null
    if ($null -ne $cell) { $f = $cell.SelectSingleNode('m:f', $ns) }
    if ($null -eq $f) {
        $rows += , @($line, $null, $null, $null, $null)
    }
    else {
        $type = $f.GetAttribute('t'); if ($type -eq '') { $type = $null }
        $ref = $f.GetAttribute('ref'); if ($ref -eq '') { $ref = $null }
        $cm = $cell.GetAttribute('cm'); if ($cm -eq '') { $cm = $null }
        $rows += , @($line, $f.InnerText, $type, $ref, $cm)
    }
    $row += 4
}
$report = [ordered]@{ excel = $version; rows = $rows }
$report | ConvertTo-Json -Depth 4 | Set-Content -Path $Out -Encoding UTF8
Write-Output "measured $($rows.Count) formulas with Excel $version"
