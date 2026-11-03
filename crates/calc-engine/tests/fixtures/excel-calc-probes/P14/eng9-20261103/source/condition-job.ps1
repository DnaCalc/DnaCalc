param($OwnedExcel,[string]$Request,[string]$Result)
$ErrorActionPreference='Stop'
$req=Get-Content -LiteralPath $Request -Raw -Encoding utf8|ConvertFrom-Json
$records=@();$fatal=$null
$null=Restore-JobBaseline
[uint32]$excelPid=0
[void][DnaSessionWindows]::GetWindowThreadProcessId([IntPtr]$OwnedExcel.Hwnd,[ref]$excelPid)
foreach($trial in $req.trials){
    try{
        $before=Start-JobTrial
        & $req.worker -OwnedExcel $OwnedExcel -Request $trial.request -Result $trial.result
        $data=Get-Content -LiteralPath $trial.result -Raw -Encoding utf8|ConvertFrom-Json
        if($data.fatal){throw $data.fatal}
        $after=Restore-JobBaseline
        $records+=@{trial=$trial.trial;result=$trial.result;result_sha256=(Get-FileHash -LiteralPath $trial.result).Hash.ToLowerInvariant();success=$true;baseline_before=$before;baseline_after=$after}
    }catch{
        $fatal=$_.Exception.ToString()
        $records+=@{trial=$trial.trial;result=$trial.result;success=$false;error=$fatal}
        break
    }
}
$value=@{fatal=$fatal;condition=$req.condition;excel_pid=$excelPid;trials=$records;requested_trials=@($req.trials).Count;
    process_scope='One fresh process per condition; workbook state and recorded baseline reset between trials. Process memory is shared and trial order is retained; these are not separate fresh-process replicates.'}
[IO.File]::WriteAllText($Result,((ConvertTo-Json -InputObject $value -Depth 40)+"`n"),[Text.UTF8Encoding]::new($false))
