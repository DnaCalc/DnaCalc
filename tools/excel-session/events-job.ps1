param($OwnedExcel,[string]$Request,[string]$Result)
$ErrorActionPreference='Stop'
$req=Get-Content -LiteralPath $Request -Raw -Encoding utf8|ConvertFrom-Json
$jobResult=$Result;$jobExcel=$OwnedExcel
$repository=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
. (Join-Path $repository 'platform/calc-oracle-excel/worker/capture-worker.ps1')
$Result=$jobResult;$excel=$jobExcel
$directory=Split-Path -Parent $Result
$book=$null;$bootstrap=$null;$fatal=$null;$trace=$null
$securityBefore=Get-ComProperty $excel 'AutomationSecurity'
try{
    if($securityBefore -ne 1){throw 'The original P06 macro fixture needs the unchanged AutomationSecurity=1 context; no setting was changed.'}
    $bootstrap=$excel.Workbooks.Add()
    $excel.Calculation=-4135;$excel.EnableEvents=$false;$excel.MultiThreadedCalculation.Enabled=$false
    $excel.Iteration=$false;$excel.CalculateBeforeSave=$false
    $controller=Join-Path $directory 'controller.xlsm'
    if((Get-FileHash -LiteralPath $req.input).Hash.ToLowerInvariant() -cne $req.input_sha256){throw 'P06 source differs from its frozen hash.'}
    Copy-Item -LiteralPath $req.input -Destination $controller
    if((Get-FileHash -LiteralPath $controller).Hash.ToLowerInvariant() -cne $req.input_sha256){throw 'P06 copy differs from its frozen hash.'}
    $book=$excel.Workbooks.Open($controller,0,$false)
    $bootstrap.Close($false);$bootstrap=$null
    $artifact=Join-Path $directory 'artifact.xlsm'
    # Run the exact saved P06 program. Its existing Stage/Snap/Typed logger retains event order.
    $trace=[string]$excel.Run(("'"+$book.Name+"'!OracleProbe.RunCase"),[string]$req.case,$artifact)
    [IO.File]::WriteAllText((Join-Path $directory 'trace.tsv'),$trace,[Text.UTF8Encoding]::new($false))
    if($trace -match "`tfault`t"){throw 'The original P06 program recorded a fault; raw trace retained.'}
    if([string]::IsNullOrEmpty($trace)){throw 'P06 returned no trace.'}
    $book.Close($false);$book=$null
}catch{$fatal=$_.Exception.ToString()}
finally{
    if($book){$book.Close($false)}
    if($bootstrap){$bootstrap.Close($false)}
}
$securityAfter=Get-ComProperty $excel 'AutomationSecurity'
if($securityAfter -ne $securityBefore){$fatal='AutomationSecurity changed unexpectedly; this result is refused.'}
$files=[ordered]@{}
Get-ChildItem -LiteralPath $directory -File|Where-Object Extension -in '.xlsm','.xlsx','.tsv'|ForEach-Object{$files[$_.Name]=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}
$output=@{fatal=$fatal;case=$req.case;trial=$req.trial;input_sha256=(Get-FileHash -LiteralPath $req.input).Hash.ToLowerInvariant();
    excel_version=[string]$excel.Version;excel_build=[string]$excel.Build;channel=(Get-Channel);locale=(Get-Culture).Name;
    automation_security_before=$securityBefore;automation_security_after=$securityAfter;files_sha256=$files;
    source='Existing committed P06 manual-false-save-log-01-tested.xlsm and its supervisor-raw.zip lineage; no VBA or registration is changed.';
    source_archive_sha256=(Get-FileHash (Join-Path $repository 'crates/calc-engine/tests/fixtures/excel-calc-probes/P06/supervisor-raw.zip')).Hash.ToLowerInvariant();
    common_source_sha256=(Get-FileHash (Join-Path $repository 'crates/calc-vba/tests/fixtures/excel-recalculation/calculation-common.bas')).Hash.ToLowerInvariant();
    probe_source_sha256=(Get-FileHash (Join-Path $repository 'crates/calc-engine/tests/fixtures/excel-calc-probes/P06/probe.bas')).Hash.ToLowerInvariant();
    limitations='P06 Save/SaveAs/SaveCopyAs controls under Manual/Except, not the new P22 Change, nested Calculate, cancel, async or RTD conditions. Saved binary provenance is retained; no source-equivalence claim is inferred from a matching filename.'}
[IO.File]::WriteAllText($Result,((ConvertTo-Json -InputObject $output -Depth 20)+"`n"),[Text.UTF8Encoding]::new($false))
