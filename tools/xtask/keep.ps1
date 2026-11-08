# Run with Start-Process pwsh -WindowStyle Hidden -ArgumentList '-NoProfile','-File',<this file>.
# Use a dedicated master clone. This is a long-lived process, not a scheduled task.
param([string]$Clone = (Join-Path $HOME 'keeper/DnaCalc'))
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$keeperRoot = Split-Path $Clone -Parent
$keeperLog = Join-Path $keeperRoot 'keeper.log'
$keeperPrevious = Join-Path $keeperRoot 'keeper.previous.log'
$keeperStable = Join-Path $keeperRoot 'xtask-stable.exe'
if (-not $env:DNACALC_AGENT_LOG) {
    $env:DNACALC_AGENT_LOG = "${env:COMPUTERNAME}:~/keeper/keeper.log"
}
[System.Diagnostics.Process]::GetCurrentProcess().PriorityClass = 'BelowNormal'
Set-Location $Clone
while ($true) {
    if ((Test-Path $keeperLog) -and (Get-Item $keeperLog).Length -gt 10MB) {
        Move-Item -Force $keeperLog $keeperPrevious
    }
    # A retained executable can report a commit that breaks the command crate itself.
    & cargo xtask keep --once >> $keeperLog 2>&1
    $keeperExit = $LASTEXITCODE
    if ($keeperExit -eq 101 -and (Test-Path $keeperStable)) {
        & $keeperStable keep --once >> $keeperLog 2>&1
    } elseif ($keeperExit -eq 0 -or $keeperExit -eq 1) {
        Copy-Item -Force 'target/xtask/debug/xtask.exe' $keeperStable
    }
    Start-Sleep -Seconds 600
}
