param([string]$Name,[switch]$Inside)
$ErrorActionPreference='Stop'
if ((Get-PSDrive C).Free -lt 25GB) { 'C: below 25 GB' | Set-Content "$PSScriptRoot/STOP"; exit 1 }
$run=Join-Path $PSScriptRoot $Name
$tools='C:/Work/DnaCalc/DnaCalc-win/tools/excel-session'
if ($Inside) {
    $params=@{Directory=$run} | ConvertTo-Json -Compress
    try {
        & "$tools/submit.ps1" -SessionDirectory "$run/session" -Script "$run/probe.ps1" -Parameters $params -Environment ww22 -TimeoutSeconds 720 -FreshRequired -NoSaveBaseline
    } finally {
        & "$tools/submit.ps1" -SessionDirectory "$run/session" -Stop
    }
} else {
    $cmd="& '$PSCommandPath' -Name '$Name' -Inside"
    & "$tools/on-private-desktop.ps1" -Command $cmd -Log "$run/private.log" -GuardOut "$run/guard.json" -SessionRoot "$run/session" -DesktopName "DnaWw22$Name"
}
