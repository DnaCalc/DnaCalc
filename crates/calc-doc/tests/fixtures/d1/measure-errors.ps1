# Measures how Excel stores each worksheet error typed into a cell as a constant, for dc-ndi (D1):
# calc_doc writes an authored error value the way Excel does.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File measure-errors.ps1 -Out <errors.json>
#
# The script starts its own Excel, types each error literal into column A (Range.Formula, as a
# user enters it) and, in column B, the formula =ERROR.TYPE(A<row>); saves the workbook as .xlsx;
# and records each A cell's t, vm and <v>, and the cached ERROR.TYPE beside it. It also records
# what the reopened workbook shows. It stops only the Excel it started.

param([Parameter(Mandatory = $true)][string]$Out)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -Namespace D1E -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$errors = @('#NULL!', '#DIV/0!', '#VALUE!', '#REF!', '#NAME?', '#NUM!', '#N/A', '#GETTING_DATA', '#SPILL!', '#CALC!', '#FIELD!', '#BLOCKED!', '#CONNECT!', '#BUSY!', '#UNKNOWN!')
$work = Join-Path ([System.IO.Path]::GetTempPath()) ('d1-errors-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
$path = Join-Path $work 'errors.xlsx'

function Start-Excel {
    $excel = New-Object -ComObject Excel.Application
    [uint32]$excelPid = 0
    [void][D1E.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
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

$entered = @{}
$started = Start-Excel
$excel = $started[0]
$version = "$($excel.Version) build $($excel.Build)"
try {
    $book = $excel.Workbooks.Add()
    $sheet = $book.Worksheets.Item(1)
    for ($i = 0; $i -lt $errors.Count; $i++) {
        $row = $i + 1
        try { $sheet.Range("A$row").Formula = $errors[$i]; $entered[$row] = [string]$sheet.Range("A$row").Text } catch { $entered[$row] = "<refused: $($_.Exception.Message)>" }
        $sheet.Range("B$row").Formula = "=ERROR.TYPE(A$row)"
    }
    $book.SaveAs($path, 51)
    $book.Close($false)
}
finally { Stop-Excel $started[0] $started[1] }

$reopened = @{}
$started = Start-Excel
try {
    $book = $started[0].Workbooks.Open($path)
    $sheet = $book.Worksheets.Item(1)
    for ($i = 0; $i -lt $errors.Count; $i++) { $row = $i + 1; $reopened[$row] = [string]$sheet.Range("A$row").Text }
    $book.Close($false)
}
finally { Stop-Excel $started[0] $started[1] }

$zip = [System.IO.Compression.ZipFile]::OpenRead($path)
try {
    $reader = New-Object System.IO.StreamReader($zip.GetEntry('xl/worksheets/sheet1.xml').Open())
    [xml]$xml = $reader.ReadToEnd()
    $reader.Close()
    $hasMetadata = $null -ne $zip.GetEntry('xl/metadata.xml')
}
finally { $zip.Dispose() }
Remove-Item $work -Recurse -Force
$ns = New-Object System.Xml.XmlNamespaceManager($xml.NameTable)
$ns.AddNamespace('m', 'http://schemas.openxmlformats.org/spreadsheetml/2006/main')
$rows = @()
for ($i = 0; $i -lt $errors.Count; $i++) {
    $row = $i + 1
    $a = $xml.SelectSingleNode("//m:c[@r='A$row']", $ns)
    $b = $xml.SelectSingleNode("//m:c[@r='B$row']", $ns)
    $av = $null; $at = $null; $avm = $null; $bv = $null
    if ($null -ne $a) {
        $at = $a.GetAttribute('t'); $avm = $a.GetAttribute('vm')
        $v = $a.SelectSingleNode('m:v', $ns); if ($null -ne $v) { $av = $v.InnerText }
        $s = $a.SelectSingleNode('m:is', $ns); if ($null -ne $s) { $av = 'inline:' + $s.InnerText }
    }
    if ($null -ne $b) { $v = $b.SelectSingleNode('m:v', $ns); if ($null -ne $v) { $bv = $v.InnerText } }
    $rows += , ([ordered]@{ typed = $errors[$i]; shown = $entered[$row]; t = $at; vm = $avm; v = $av; error_type = $bv; reopened = $reopened[$row] })
}
$report = [ordered]@{ excel = $version; metadata_part = $hasMetadata; rows = $rows }
$report | ConvertTo-Json -Depth 4 | Set-Content -Path $Out -Encoding UTF8
$report | ConvertTo-Json -Depth 4
