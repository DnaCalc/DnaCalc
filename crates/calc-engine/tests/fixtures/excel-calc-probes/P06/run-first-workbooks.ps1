param([int]$Repetitions=10,[switch]$Resume)
$ErrorActionPreference='Stop'
$driver=Join-Path $PSScriptRoot '../../../../../calc-vba/tests/fixtures/excel-recalculation/run-calculation-probe.ps1'
$driver=(Resolve-Path -LiteralPath $driver).Path
$records=@()
if($Resume -and (Test-Path (Join-Path $PSScriptRoot 'first-open-summary.json'))){$records=@(Get-Content (Join-Path $PSScriptRoot 'first-open-summary.json') -Raw|ConvertFrom-Json)}
foreach($mode in @('manual','auto')){
    $inputName=if($mode -eq 'manual'){'manual-false-save-log-01-tested.xlsm'}else{'synthetic-first-open-auto.xlsm'}
    for($i=1;$i -le $Repetitions;$i++){
        $base='P06-first-'+$mode+'-'+$i.ToString('00')
        if(@($records|Where-Object {$_.run_id -like ($base+'*-safe')}).Count -gt 0){continue}
        $id=$base+'-safe'
        $retry=0
        while(Test-Path (Join-Path $PSScriptRoot ('raw/'+$id))){$retry++;$id=$base+'-retry'+$retry+'-safe'}
        & $driver -ProbeDirectory $PSScriptRoot -SourceFile (Join-Path $PSScriptRoot 'first-open-controller.bas') -FirstWorkbook (Join-Path $PSScriptRoot $inputName) -Repetitions 1 -ArtifactPrefix ('first-'+$mode+'-'+$i.ToString('00')+'-') -RunId $id -SafeMode
        $raw=Join-Path $PSScriptRoot ('raw/'+$id)
        $result=Get-Content (Join-Path $raw 'results.json') -Raw|ConvertFrom-Json
        $observation=Get-Content (Join-Path $raw 'windows-scalar-interop/first-open-observation.json') -Raw|ConvertFrom-Json
        $traceName='first-'+$mode+'-'+$i.ToString('00')+'-trace.tsv'
        Copy-Item -LiteralPath (Join-Path $raw 'windows-scalar-interop/first-open-trace.tsv') -Destination (Join-Path $PSScriptRoot $traceName)
        $case=$result.cases[0]
        $records += @{run_id=$id;profile=$mode;observation=$observation;trace=$traceName;compile=$case.compile_status;run=$case.run_status;cleanup=$case.cleanup_status;transport=$case.transport_error}
        $records|ConvertTo-Json -Depth 12|Set-Content (Join-Path $PSScriptRoot 'first-open-summary.json') -Encoding utf8NoBOM
        if($case.compile_status -ne 'ok' -or $case.run_status -ne 'ok' -or $case.cleanup_status -ne 'owned-process-zero' -or $observation.sha256_before -cne $observation.sha256_after){throw ('First-workbook measurement incomplete: '+$id)}
    }
}
