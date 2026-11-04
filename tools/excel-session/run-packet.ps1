param([Parameter(Mandatory)][string]$PacketDirectory, [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$DnaCalcCheckout=(Join-Path $PSScriptRoot '../..'),
    [string]$EvidenceCheckout='C:\Work\DnaCalc\OxFunc-Evidence', [switch]$CapturePending,
    [string]$XtaskExecutable, [string[]]$Functions, [switch]$AllowRepeatedCalls)
# Generalizes the supplied R16 RUN-R16.ps1 using the same witness and catalogue routes.
$ErrorActionPreference='Stop'
$packet=(Resolve-Path -LiteralPath $PacketDirectory).Path
$checkout=(Resolve-Path -LiteralPath $DnaCalcCheckout).Path
$evidence=(Resolve-Path -LiteralPath $EvidenceCheckout).Path
$output=[IO.Path]::GetFullPath($OutputDirectory)
if(Test-Path -LiteralPath $output){throw 'Use a new output directory; preserve earlier attempts.'}
foreach($protected in @($packet,$checkout,$evidence)) {
    if($output -ieq $protected -or $output.StartsWith($protected+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Output must be outside the packet and checkouts.'}
}
[void][IO.Directory]::CreateDirectory($output)
if($XtaskExecutable){$XtaskExecutable=(Resolve-Path -LiteralPath $XtaskExecutable).Path}
function Write-Json($Path,$Value){[IO.File]::WriteAllText($Path,((ConvertTo-Json -InputObject $Value -Depth 40)+"`n"),[Text.UTF8Encoding]::new($false))}
function Hashes($Root){$h=[ordered]@{}; Get-ChildItem -LiteralPath $Root -Recurse -File|ForEach-Object{$h[[IO.Path]::GetRelativePath($Root,$_.FullName).Replace('\','/')]=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()};return $h}
function Invoke-RecordedCargo([string[]]$Arguments,[string]$Log){
    if($XtaskExecutable){
        $harnessArguments=@($Arguments|Select-Object -Skip 1)
        & $XtaskExecutable @harnessArguments *> $Log
    }else{& cargo @Arguments *> $Log}
    if($LASTEXITCODE -ne 0){throw "cargo $($Arguments -join ' ') exited $LASTEXITCODE; see $Log"}
}
$manifestPath=Join-Path $packet 'manifest.json'
if(-not(Test-Path -LiteralPath $manifestPath)){$manifestPath=Join-Path $packet 'packet-manifest.json'}
$manifest=Get-Content -LiteralPath $manifestPath -Raw -Encoding utf8|ConvertFrom-Json
if($manifest.PSObject.Properties['files_sha256']) {
    foreach($entry in $manifest.files_sha256.PSObject.Properties){if((Get-FileHash -LiteralPath (Join-Path $packet $entry.Name)).Hash.ToLowerInvariant() -cne $entry.Value){throw "Packet hash differs: $($entry.Name)"}}
} elseif($manifest.PSObject.Properties['files']) {
    if($manifest.files -is [array]){
        foreach($entry in $manifest.files){if((Get-FileHash -LiteralPath (Join-Path $packet $entry.path)).Hash.ToLowerInvariant() -cne $entry.sha256){throw "Packet hash differs: $($entry.path)"}}
    }else{
        foreach($entry in $manifest.files.PSObject.Properties){if((Get-FileHash -LiteralPath (Join-Path $packet $entry.Name)).Hash.ToLowerInvariant() -cne $entry.Value){throw "Packet hash differs: $($entry.Name)"}}
    }
}
Copy-Item -LiteralPath $packet -Destination (Join-Path $output 'packet') -Recurse
if($XtaskExecutable){Write-Json (Join-Path $output 'command-executable.json') @{path=$XtaskExecutable;sha256=(Get-FileHash -LiteralPath $XtaskExecutable).Hash.ToLowerInvariant();revision=(& git -C $checkout rev-parse HEAD)}}
Write-Json (Join-Path $output 'packet-sha256.json') (Hashes $packet)
if(Test-Path -LiteralPath (Join-Path $packet 'batches.json')){$batches=@(Get-Content -LiteralPath (Join-Path $packet 'batches.json') -Raw|ConvertFrom-Json)}
else{$batches=@(Get-ChildItem -LiteralPath $packet -Filter '*.inputs.json'|ForEach-Object{[pscustomobject]@{function=$_.Name.Replace('.inputs.json','');path=$_.Name}})}
$packetBatchCount=$batches.Count
if($Functions){
    foreach($name in $Functions){if($name -notin @($batches.function)){throw "Function is absent from packet: $name"}}
    $batches=@($batches|Where-Object{$_.function -in $Functions})
}
$wanted=@{}
foreach($batch in $batches){$wanted[(Get-FileHash -LiteralPath (Join-Path $packet $batch.path)).Hash.ToLowerInvariant()]=$batch.function}
$roots=[ordered]@{'worker/capture'=(Join-Path $checkout 'target/c');'worker/read'=(Join-Path $checkout 'target/witness/read');'worker/frozen'=(Join-Path $checkout 'target/witness/frozen');'OxFunc-Evidence/readings'=(Join-Path $evidence 'readings')}
$before=@{}
foreach($name in $roots.Keys){$before[$name]=@{};if(Test-Path $roots[$name]){Get-ChildItem -LiteralPath $roots[$name] -Recurse -File|ForEach-Object{$before[$name][$_.FullName]=(Get-FileHash -LiteralPath $_.FullName).Hash}}}
if(-not $env:DNACALC_EXCEL_SESSION){$env:DNACALC_EXCEL_SESSION=Join-Path $checkout 'target/excel-session'}
$session=[IO.Path]::GetFullPath($env:DNACALC_EXCEL_SESSION)
$receiptsBefore=@{};if(Test-Path (Join-Path $session 'completed')){Get-ChildItem (Join-Path $session 'completed') -Filter receipt.json -Recurse|ForEach-Object{$receiptsBefore[$_.FullName]=$true}}
$oldLegacy=$env:DNACALC_LEGACY_ROOT
$oldPacket=$env:DNACALC_EXCEL_PACKET
$failure=$null;$completed=@();$batchFailures=@()
Push-Location $checkout
try {
    $env:DNACALC_LEGACY_ROOT=Split-Path -Parent $evidence
    $env:DNACALC_EXCEL_PACKET=$output
    if($CapturePending){Invoke-RecordedCargo @('xtask','capture','--pending','--timeout','900','--attempts','1') (Join-Path $output 'capture-console.txt')}
    foreach($batch in $batches){
        try{
            $arguments=@('xtask','witness','read','--func',$batch.function,'--inputs',(Join-Path $packet $batch.path),'--timeout','900')
            if($AllowRepeatedCalls){$arguments+='--repeat-calls'}
            Invoke-RecordedCargo $arguments (Join-Path $output ('read-'+$batch.function+'.txt'))
            $completed+= $batch.function
        }catch{$batchFailures+=@{function=$batch.function;error=$_.ToString()}}
    }
}catch{$failure=$_.ToString()}
finally {
    Pop-Location
    $env:DNACALC_LEGACY_ROOT=$oldLegacy
    $env:DNACALC_EXCEL_PACKET=$oldPacket
    foreach($name in $roots.Keys){
        if(-not(Test-Path $roots[$name])){continue}
        Get-ChildItem -LiteralPath $roots[$name] -Recurse -File|Where-Object{-not $before[$name].ContainsKey($_.FullName) -or $before[$name][$_.FullName] -ne (Get-FileHash -LiteralPath $_.FullName).Hash}|ForEach-Object{
            $dest=Join-Path (Join-Path $output $name) ([IO.Path]::GetRelativePath($roots[$name],$_.FullName))
            [void][IO.Directory]::CreateDirectory((Split-Path -Parent $dest));Copy-Item -LiteralPath $_.FullName -Destination $dest
        }
    }
    $receipts=@();$launches=@{}
    if(Test-Path (Join-Path $session 'completed')){Get-ChildItem (Join-Path $session 'completed') -Filter receipt.json -Recurse|Where-Object{-not $receiptsBefore.ContainsKey($_.FullName)}|ForEach-Object{
        $r=Get-Content $_.FullName -Raw|ConvertFrom-Json
        if($r.job.packet_id -cne $output){return}
        $receipts+=$r
        for($n=$r.launches_before+1;$n -le $r.launches_after;$n++){$launches[$n]=$true}
        $parent=Join-Path $output 'session';[void][IO.Directory]::CreateDirectory($parent);Copy-Item -LiteralPath $_.Directory.FullName -Destination $parent -Recurse
    }}
    if(Test-Path (Join-Path $session 'launches')){Copy-Item -LiteralPath (Join-Path $session 'launches') -Destination (Join-Path $output 'session-launches') -Recurse}
    $canonical=@();$counts=@{};$unread=0
    $readingOutput=Join-Path $output 'OxFunc-Evidence/readings'
    if(Test-Path $readingOutput){Get-ChildItem -LiteralPath $readingOutput -Filter '*.json' -Recurse|ForEach-Object{
        $doc=Get-Content -LiteralPath $_.FullName -Raw -Encoding utf8|ConvertFrom-Json
        if($doc.PSObject.Properties['capture_provenance'] -and $doc.capture_provenance.PSObject.Properties['frozen_model_predictions']){
            $hash=[string]$doc.capture_provenance.frozen_model_predictions.inputs_sha256
            if($wanted.ContainsKey($hash)){
                if(-not $counts.ContainsKey($hash)){$counts[$hash]=0};$counts[$hash]++
                $unread+=@($doc.unread).Count
                $canonical+=@{path=[IO.Path]::GetRelativePath($output,$_.FullName);function=$doc.function;inputs_sha256=$hash;calls=@($doc.witnesses).Count;unread=@($doc.unread).Count}
            }
        }
    }}
    $coverageFailed=$counts.Count -ne $wanted.Count -or @($counts.Values|Where-Object{$_ -ne 1}).Count -gt 0 -or $unread -gt 0
    Write-Json (Join-Path $output 'run-status.json') @{failed=($null -ne $failure -or $batchFailures.Count -gt 0 -or $coverageFailed);command_failure=$failure;batch_failures=$batchFailures;packet_batches=$packetBatchCount;requested_batches=$batches.Count;completed_functions=$completed;canonical=$canonical;unread_calls=$unread;excel_launches=$launches.Count;jobs=$receipts.Count;session_directory=$session;limits='Raw answers and frozen predictions remain separate. Unread calls and unrun conditions are not passes.'}
    Write-Json (Join-Path $output 'results-sha256.json') (Hashes $output)
}
if($failure){throw $failure}
if($batchFailures.Count -gt 0 -or $coverageFailed){throw 'Packet has failed, missing, duplicate or unread batches; all attempts are retained.'}
Write-Output "Packet retained at $output; Excel launches: $($launches.Count)"
