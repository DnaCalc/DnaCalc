# Makes, in Excel, an .xlsb and an .xlsx of each workbook of a folder (dc-5sr: reading Excel
# binary workbooks): each workbook opened read-only (links never updated, macros not run, alerts
# off), saved as an Excel binary workbook (FileFormat 50) and again as a workbook (51, or 52 for one
# with macros), so the two hold what Excel itself holds of it and calc-doc's models of the twins
# can be compared. A workbook Excel does not open is listed as such.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-twins.ps1 -Dirs '<a>;<b>' -Out <dir>
#
# One Excel instance, which this script starts and stops itself (by the PID of its own window).
# A busy Excel (RPC_E_CALL_REJECTED, RPC_E_SERVERCALL_RETRYLATER) is waited for and asked again;
# one that stops answering is replaced.

param(
    [Parameter(Mandatory = $true)][string]$Dirs,
    [Parameter(Mandatory = $true)][string]$Out
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -Namespace D5S -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$script:excel = $null
$script:excelPid = 0
function Start-Excel {
    $script:excel = New-Object -ComObject Excel.Application
    [uint32]$id = 0
    [void][D5S.Win]::GetWindowThreadProcessId([System.IntPtr]$script:excel.Hwnd, [ref]$id)
    $script:excelPid = $id
    $script:excel.Visible = $false
    $script:excel.DisplayAlerts = $false
    $script:excel.AskToUpdateLinks = $false
    $script:excel.AutomationSecurity = 3
}
function Stop-Excel {
    if ($null -ne $script:excel) {
        try { $script:excel.Quit() } catch {}
        try { [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($script:excel) } catch {}
    }
    $script:excel = $null
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    if ($script:excelPid -ne 0) { Get-Process -Id $script:excelPid -ErrorAction SilentlyContinue | Stop-Process -Force }
    $script:excelPid = 0
}
function Is-Busy($exception) {
    $inner = $exception
    while ($null -ne $inner) {
        if ($inner -is [System.Runtime.InteropServices.COMException] -and ($inner.ErrorCode -eq -2147418111 -or $inner.ErrorCode -eq -2147417846)) { return $true }
        $inner = $inner.InnerException
    }
    return $false
}
function Twin([string]$path, [string]$stem, [bool]$macros) {
    $missing = [System.Type]::Missing
    $arguments = [object[]]@($path, 0, $true, $missing, 'dnacalc-no-password', $missing, $true, $missing, $missing, $false, $false, $missing, $false, $false, 0)
    $book = [System.__ComObject].InvokeMember('Open', [System.Reflection.BindingFlags]::InvokeMethod, $null, $script:excel.Workbooks, $arguments)
    try {
        $book.SaveAs((Join-Path $Out ($stem + '.xlsb')), 50)
        $xlsx = if ($macros) { ($stem + '.xlsm') } else { ($stem + '.xlsx') }
        $book.SaveAs((Join-Path $Out $xlsx), $(if ($macros) { 52 } else { 51 }))
    }
    finally { $book.Close($false); [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($book) }
}

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$files = @()
foreach ($dir in $Dirs.Split(';')) {
    if ($dir -eq '') { continue }
    $files += Get-ChildItem -LiteralPath $dir -File | Where-Object { $_.Extension -match '^\.(xlsx|xlsm)$' -and -not $_.Name.StartsWith('~$') } | Sort-Object FullName
}
$results = New-Object System.Collections.ArrayList
$version = $null
Start-Excel
try {
    $version = "$($script:excel.Version) build $($script:excel.Build)"
    foreach ($file in $files) {
        $stem = [System.IO.Path]::GetFileNameWithoutExtension($file.Name)
        $macros = $file.Extension -ieq '.xlsm'
        $row = [ordered]@{ source = $file.Name; twinned = $false; error = $null }
        for ($attempt = 1; $attempt -le 6; $attempt++) {
            try { Twin $file.FullName $stem $macros; $row.twinned = $true; $row.error = $null; break }
            catch {
                $row.error = $_.Exception.Message
                if (Is-Busy $_.Exception) { Start-Sleep -Seconds 2; continue }
                $alive = $true
                try { [void]$script:excel.Version } catch { $alive = $false }
                if (-not $alive -and $attempt -lt 3) { Stop-Excel; Start-Excel; continue }
                break
            }
        }
        [void]$results.Add($row)
    }
}
finally { Stop-Excel }
[ordered]@{ excel = $version; files = $files.Count; twinned = @($results | Where-Object { $_.twinned }).Count; results = $results } |
    ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 (Join-Path $Out 'twins.json')
Write-Output ("twinned {0} of {1}" -f @($results | Where-Object { $_.twinned }).Count, $files.Count)
