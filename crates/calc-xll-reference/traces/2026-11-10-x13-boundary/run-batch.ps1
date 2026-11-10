# dc-6332.10 boundary batch (answer 13: D.3, D.9 sentinel pass, D.22 rows 16385/16391/16403).
# One untraced Excel control of fixture/x13-boundary-cases.json through run.ps1 (the lane's owned
# session, tools/excel-session), then one OneCalc host process per case or case group, because the
# host's case runner stops at the first refusal. Run it under tools/excel-session/on-private-desktop.ps1.
param([Parameter(Mandatory)][string]$RunRoot,[Parameter(Mandatory)][string]$AddInDirectory,
      [Parameter(Mandatory)][string]$HostExecutable,[string]$Cases,[switch]$HostOnly,[switch]$ExcelOnly,[switch]$Direct)
# -Direct starts the host executable itself for each group (the same exe and case route that
# run.ps1 starts, without its per-run source hashing): host.stdout, host.stderr, host.exit.json and
# a small environment.json with the add-in file hashes and the executable's hash.
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$false
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
$runner=Join-Path $repo 'crates/calc-xll-reference/run.ps1'
$casesPath=if($Cases){(Resolve-Path -LiteralPath $Cases).Path}else{Join-Path $repo 'crates/calc-xll-reference/fixture/x13-boundary-cases.json'}
New-Item -ItemType Directory -Force -Path $RunRoot | Out-Null
if(-not $HostOnly){
    & pwsh -NoProfile -Sta -File $runner -NoTrace -RunDirectory (Join-Path $RunRoot 'excel') -AddInDirectory $AddInDirectory -XllFileName X1.xll -Cases $casesPath *> (Join-Path $RunRoot 'excel.log')
    [ordered]@{side='excel';exit=$LASTEXITCODE} | ConvertTo-Json -Compress | Add-Content -LiteralPath (Join-Path $RunRoot 'runner.jsonl')
}
if($ExcelOnly){ return }
# Not `$cases`: PowerShell names are case-insensitive and `$Cases` is the typed [string] parameter.
$caseList=@((Get-Content -LiteralPath $casesPath -Raw | ConvertFrom-Json) | ForEach-Object { $_ })
$groups=[Collections.Generic.List[object]]::new()
foreach($case in $caseList){
    $key=if($case.PSObject.Properties['group']){[string]$case.group}else{$null}
    if($key -and $groups.Count -and $groups[$groups.Count-1].key -eq $key){ $groups[$groups.Count-1].cases.Add($case) }
    else { $list=[Collections.Generic.List[object]]::new(); $list.Add($case); $groups.Add([pscustomobject]@{key=$key;cases=$list}) }
}
New-Item -ItemType Directory -Force -Path (Join-Path $RunRoot 'hosts/cases') | Out-Null
for($index=0;$index -lt $groups.Count;$index++){
    $name=$index.ToString('D3')
    $packet=Join-Path $RunRoot ('hosts/cases/'+$name+'.json')
    ConvertTo-Json -InputObject @($groups[$index].cases) -Depth 6 | Set-Content -LiteralPath $packet -Encoding utf8NoBOM
    if($Direct){
        $run=Join-Path $RunRoot ('hosts/'+$name); New-Item -ItemType Directory -Force -Path $run | Out-Null
        $files=[ordered]@{}; Get-ChildItem -LiteralPath $AddInDirectory -File | ForEach-Object { $files[$_.Name]=(Get-FileHash -LiteralPath $_.FullName).Hash }
        [ordered]@{fixture_files=$files;executable_sha256=(Get-FileHash -LiteralPath $HostExecutable).Hash;mode='direct host run'} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $run 'environment.json') -Encoding utf8NoBOM
        & $HostExecutable --xll (Join-Path $AddInDirectory 'X1.xll') --cases $packet --sheet X0 > (Join-Path $run 'host.stdout') 2> (Join-Path $run 'host.stderr')
        $code=$LASTEXITCODE
        [ordered]@{exit_code=$code;finished=($code -eq 0)} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $run 'host.exit.json') -Encoding utf8NoBOM
    } else {
    & pwsh -NoProfile -Sta -File $runner -NoTrace -RunDirectory (Join-Path $RunRoot ('hosts/'+$name)) -AddInDirectory $AddInDirectory -XllFileName X1.xll -Cases $packet -HostExecutable $HostExecutable *> (Join-Path $RunRoot ('hosts/'+$name+'.log'))
    }
    [ordered]@{side='host';index=$index;formulas=@($groups[$index].cases|ForEach-Object formula);exit=$LASTEXITCODE} | ConvertTo-Json -Compress | Add-Content -LiteralPath (Join-Path $RunRoot 'runner.jsonl')
}
