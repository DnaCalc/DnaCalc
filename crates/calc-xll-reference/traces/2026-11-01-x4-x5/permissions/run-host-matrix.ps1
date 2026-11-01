param([int]$Parity,[string]$RunRoot)
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$false
$root=$RunRoot
New-Item -ItemType Directory -Force -Path (Join-Path $root 'cases') | Out-Null
$cases=Get-Content crates/calc-xll-reference/fixture/x5-permission-cases.json -Raw | ConvertFrom-Json
for($index=0;$index -lt $cases.Count;$index++){
 if($index%2 -ne $Parity){continue}
 $name=$index.ToString('D3')
 $packet=Join-Path $root ('cases/'+$name+'.json')
 ConvertTo-Json -InputObject @($cases[$index]) -Depth 6 | Set-Content -LiteralPath $packet
 $run=Join-Path $root $name
 & pwsh -NoProfile -Sta -File crates/calc-xll-reference/run.ps1 -RunDirectory $run -AddInDirectory C:\Work\DnaCalc\codex-runs\x5-upstream\native-rtd-runtime -XllFileName X5Native64.xll -Cases $packet -HostExecutable target/xtask-accept/debug/dna-calc-desktop.exe -Engine -NoTrace *> (Join-Path $root ($name+'.log'))
 [ordered]@{index=$index;formula=$cases[$index].formula;controller_exit=$LASTEXITCODE} | ConvertTo-Json -Compress | Add-Content -LiteralPath (Join-Path $root ('runner-'+$Parity+'.jsonl'))
}

