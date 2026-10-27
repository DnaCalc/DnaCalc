# Opens each workbook of one or more folders in Excel (dc-1yp: document fidelity, phase 2) and
# records whether it opens with a normal load, that is without repair: Workbooks.Open with
# CorruptLoad 0 (xlNormalLoad), which fails rather than repairs, read-only, links never updated,
# alerts off, macros not run (Application.AutomationSecurity 3, msoAutomationSecurityForceDisable,
# a setting of this Excel instance only), and a password that cannot be the workbook's so an
# encrypted one fails instead of prompting.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File open-in-excel.ps1 -Dirs '<a>;<b>' -Out <json>
#
# One Excel instance at a time, which this script starts and stops itself (by the PID of its own
# window, never another Excel). A call Excel rejects because it is busy (RPC_E_CALL_REJECTED,
# RPC_E_SERVERCALL_RETRYLATER) is retried; an Excel that stops answering is replaced and the
# workbook tried once more.

param(
    [Parameter(Mandatory = $true)][string]$Dirs,
    [Parameter(Mandatory = $true)][string]$Out
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
Add-Type -Namespace D1Y -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'

$script:excel = $null
$script:excelPid = 0

function Start-Excel {
    $script:excel = New-Object -ComObject Excel.Application
    [uint32]$id = 0
    [void][D1Y.Win]::GetWindowThreadProcessId([System.IntPtr]$script:excel.Hwnd, [ref]$id)
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
    $code = 0
    $inner = $exception
    while ($null -ne $inner) {
        if ($inner -is [System.Runtime.InteropServices.COMException]) { $code = $inner.ErrorCode }
        $inner = $inner.InnerException
    }
    return ($code -eq -2147418111 -or $code -eq -2147417846)
}

function Try-Open([string]$path) {
    $missing = [System.Type]::Missing
    $arguments = [object[]]@($path, 0, $true, $missing, 'dnacalc-no-password', $missing, $true, $missing, $missing, $false, $false, $missing, $false, $false, 0)
    $book = [System.__ComObject].InvokeMember('Open', [System.Reflection.BindingFlags]::InvokeMethod, $null, $script:excel.Workbooks, $arguments)
    $name = [string]$book.Name
    $book.Close($false)
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($book)
    return $name
}

$files = @()
foreach ($dir in $Dirs.Split(';')) {
    if ($dir -eq '') { continue }
    $files += Get-ChildItem -LiteralPath $dir -File | Where-Object { $_.Extension -match '^\.(xlsx|xlsm|xlsb)$' -and -not $_.Name.StartsWith('~$') } | Sort-Object FullName
}

$results = New-Object System.Collections.ArrayList
$version = $null
Start-Excel
try {
    $version = "$($script:excel.Version) build $($script:excel.Build)"
    $index = 0
    foreach ($file in $files) {
        $index += 1
        $row = [ordered]@{ path = $file.FullName; opened = $false; name = $null; error = $null; attempts = 0 }
        for ($attempt = 1; $attempt -le 6; $attempt++) {
            $row.attempts = $attempt
            try {
                $row.name = Try-Open $file.FullName
                $row.opened = $true
                $row.error = $null
                break
            } catch {
                $row.error = $_.Exception.Message
                if (Is-Busy $_.Exception) { Start-Sleep -Seconds 2; continue }
                # An Excel that no longer answers is replaced and the workbook tried again once.
                $alive = $true
                try { [void]$script:excel.Version } catch { $alive = $false }
                if (-not $alive -and $attempt -lt 3) { Stop-Excel; Start-Excel; continue }
                break
            }
        }
        [void]$results.Add($row)
        if ($index % 25 -eq 0) { Write-Output "$index of $($files.Count)" }
    }
}
finally { Stop-Excel }

[ordered]@{
    excel = $version
    files = $files.Count
    opened = @($results | Where-Object { $_.opened }).Count
    results = $results
} | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 $Out
Write-Output ("opened {0} of {1}" -f @($results | Where-Object { $_.opened }).Count, $files.Count)
