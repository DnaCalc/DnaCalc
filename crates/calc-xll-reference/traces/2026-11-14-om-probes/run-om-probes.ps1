# dc-1nbp.6 (14.e): runs om-probes-job.ps1 in fresh owned Excel processes through the lane's
# Excel session host. Run it on a private desktop so no Excel window reaches the owner's desktop:
#   pwsh -File tools/excel-session/on-private-desktop.ps1 -Command "pwsh -NoProfile -File <this> -Root <dir>" `
#     -Log <dir>/driver.log -GuardOut <dir>/guard.json -SessionRoot <dir> -DesktopName DnaCalcOmProbes
# Build the add-in first: dotnet build crates/calc-xll-reference/fixture/X19Com.csproj -o <dir>/build
param([Parameter(Mandatory)][string]$Root, [int]$Trials = 3)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
$submit = Join-Path $repo 'tools/excel-session/submit.ps1'
$job = Join-Path $PSScriptRoot 'om-probes-job.ps1'
$session = Join-Path $Root 'session'
for ($i = 1; $i -le $Trials; $i++) {
    $runDir = Join-Path $Root ("trial-$i")
    New-Item -ItemType Directory -Force -Path $runDir | Out-Null
    $request = Join-Path $runDir 'request.json'
    @{ build_dir = (Join-Path $Root 'build'); run_dir = $runDir; trial = $i } | ConvertTo-Json | Set-Content -LiteralPath $request -Encoding utf8NoBOM
    $parameters = @{ Request = $request; Result = (Join-Path $runDir 'result.json') } | ConvertTo-Json -Compress
    # Registering an XLL changes the registered-function baseline, so every trial needs a new process.
    # The host then refuses the job for baseline drift after reset (the add-in stays registered) and recycles
    # the process; the job's own outputs are complete by then and are what this probe records.
    try { & $submit -SessionDirectory $session -Script $job -Parameters $parameters -Environment 'om-probes' -FreshRequired -TimeoutSeconds 600 }
    catch { Write-Output ("trial $i host verdict: " + $_.Exception.Message) }
}
& $submit -SessionDirectory $session -Stop
