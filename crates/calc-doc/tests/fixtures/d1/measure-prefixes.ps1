# Measures the prefix Excel writes before each worksheet function's name in a saved package, for
# dc-ndi (D1): calc_doc writes authored formulas in that file form.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File measure-prefixes.ps1 -Functions <functions.json> -Out <prefixes.json>
#
# <functions.json> lists [name, required argument count] pairs. The script starts its own Excel,
# enters one formula per function in column A of a new workbook through Range.Formula2 (the
# function called with its required number of arguments, each the number 1, retried with up to
# eight more when Excel refuses the entry, then with each argument the range Z1:Z2, which a
# reference parameter accepts), saves the workbook as .xlsx, and reads each cell's
# <f> text from the saved worksheet part. Each result row is [name, entered formula, stored <f>
# text], or [name, null, null] when Excel refused every entry. It stops only the Excel it started.

param(
    [Parameter(Mandatory = $true)][string]$Functions,
    [Parameter(Mandatory = $true)][string]$Out
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -Namespace D1P -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$list = Get-Content -Raw $Functions | ConvertFrom-Json
$work = Join-Path ([System.IO.Path]::GetTempPath()) ('d1-prefixes-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
$path = Join-Path $work 'prefixes.xlsx'

$entered = @{}
$excel = New-Object -ComObject Excel.Application
[uint32]$excelPid = 0
[void][D1P.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$excelPid)
$version = "$($excel.Version) build $($excel.Build)"
try {
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $book = $excel.Workbooks.Add()
    $excel.Calculation = -4135  # manual: the formulas need not calculate
    $sheet = $book.Worksheets.Item(1)
    $row = 0
    foreach ($pair in $list) {
        $row++
        $name = [string]$pair[0]
        $required = [int]$pair[1]
        $candidates = @()
        switch ($name) {
            'LET' { $candidates = @('=LET(x,1,x)') }
            'LAMBDA' { $candidates = @('=LAMBDA(x,x)(1)') }
            default {
                foreach ($argument in @('1', 'Z1:Z2')) {
                    for ($extra = 0; $extra -le 8; $extra++) {
                        $count = $required + $extra
                        $candidates += ('=' + $name + '(' + ((@($argument) * $count) -join ',') + ')')
                    }
                }
            }
        }
        foreach ($candidate in $candidates) {
            try {
                $sheet.Range("A$row").Formula2 = $candidate
                $entered[$name] = $candidate
                break
            }
            catch { }
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
    $entry = $zip.GetEntry('xl/worksheets/sheet1.xml')
    $reader = New-Object System.IO.StreamReader($entry.Open())
    [xml]$xml = $reader.ReadToEnd()
    $reader.Close()
}
finally { $zip.Dispose() }
$ns = New-Object System.Xml.XmlNamespaceManager($xml.NameTable)
$ns.AddNamespace('m', 'http://schemas.openxmlformats.org/spreadsheetml/2006/main')
$stored = @{}
foreach ($cell in $xml.SelectNodes('//m:c', $ns)) {
    $f = $cell.SelectSingleNode('m:f', $ns)
    if ($null -ne $f) { $stored[$cell.GetAttribute('r')] = $f.InnerText }
}
Remove-Item $work -Recurse -Force

$rows = @()
$row = 0
foreach ($pair in $list) {
    $row++
    $name = [string]$pair[0]
    $text = $null
    if ($stored.ContainsKey("A$row")) { $text = $stored["A$row"] }
    $rows += , @($name, $entered[$name], $text)
}
$report = [ordered]@{ excel = $version; rows = $rows }
$report | ConvertTo-Json -Depth 4 | Set-Content -Path $Out -Encoding UTF8
Write-Output "measured $($rows.Count) functions with Excel $version"
