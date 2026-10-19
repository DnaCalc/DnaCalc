# A stand-in for the capture worker, for the windows part of infra_m1_2__untrusted_workbook.
#
# With -Nonce and -StartedFile it starts a stand-in for Excel through WMI (Win32_Process.Create),
# so that, like an Excel that COM activates, the stand-in is not a descendant of this worker: its
# parent is the WMI provider host. The stand-in is a PowerShell process whose command line carries
# -Nonce, which writes its own PID to -StartedFile and then sleeps. This worker reports that process
# on the worker protocol as the Excel it started.
#
# With -ReportPid it starts nothing and reports that PID instead, standing for a worker that names
# an Excel the capture did not start.
#
# It then hangs (-Hang) or exits at once, leaving any stand-in running. The adapter's isolation is
# what the case tests: this script never terminates anything itself.
param(
    [string]$Nonce,
    [string]$StartedFile,
    [int]$ReportPid = 0,
    [switch]$Hang
)
$ErrorActionPreference = 'Stop'
if ($ReportPid -ne 0) {
    $reported = $ReportPid
} else {
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $command = "`$null = '$Nonce'; Set-Content -LiteralPath '$StartedFile' -Value `$PID; Start-Sleep -Seconds 600"
    $line = "`"$powershell`" -NoProfile -NonInteractive -WindowStyle Hidden -Command `"$command`""
    $created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $line }
    if ($created.ReturnValue -ne 0) { throw "Win32_Process.Create returned $($created.ReturnValue)" }
    $reported = [int]$created.ProcessId
    $deadline = (Get-Date).AddSeconds(60)
    while (-not (Test-Path -LiteralPath $StartedFile) -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
}
[Console]::Out.WriteLine("dnacalc-capture-worker/1 excel-pid $reported")
[Console]::Out.Flush()
if ($Hang) {
    Start-Sleep -Seconds 600
}
exit 0
