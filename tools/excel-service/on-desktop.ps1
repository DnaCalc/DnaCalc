# Runs a command on the v2 private desktop (DnaCalcExcelV2) so every Excel it starts stays off the owner's desktop.
# Writes <Out>.log and <Out>.guard.json; the guard's windows_on_user_desktop must read 0.
param([Parameter(Mandatory)][string]$Command,[Parameter(Mandatory)][string]$Out,[string]$State='C:\Work\DnaCalc\codex-runs\excel-service-v2',[string]$DesktopName='DnaCalcExcelV2')
$ErrorActionPreference='Stop'
$guard=Join-Path $PSScriptRoot '../excel-session/on-private-desktop.ps1'
& pwsh -NoProfile -File $guard -Command $Command -Log "$Out.log" -GuardOut "$Out.guard.json" -SessionRoot (Join-Path $State 'excel') -DesktopName $DesktopName
exit $LASTEXITCODE
