param($OwnedExcel,[string]$Dir,[string]$Result)
# Measures, in the owned Excel session, how Excel identifies an RTD topic. The X5 native server
# (private HKCU class DnaCalc.X5.<excel pid>, removed by its own xlAutoClose) logs every
# ConnectData call with Excel's topic id to rtd.tsv beside the loaded module. Cases count the
# ConnectData calls caused by formulas that differ in one way; one call for two formulas means
# Excel treats them as one topic.
$ErrorActionPreference='Stop'
[uint32]$excelPid=0
[void][DnaSessionWindows]::GetWindowThreadProcessId([IntPtr]$OwnedExcel.Hwnd,[ref]$excelPid)
$log=Join-Path $Dir 'rtd.tsv'
$xll=Join-Path $Dir 'X5Native64.xll'
$records=@();$fatal=$null
function Lines { if(Test-Path -LiteralPath $log){@(Get-Content -LiteralPath $log -Encoding utf8)}else{@()} }
function Pump($seconds){$end=[DateTime]::UtcNow.AddSeconds($seconds);while([DateTime]::UtcNow -lt $end){[void]$OwnedExcel.Ready;Start-Sleep -Milliseconds 50}}
try{
    $progid="DnaCalc.X5.$excelPid"
    $registered=[bool]$OwnedExcel.RegisterXLL($xll)
    $book=$OwnedExcel.Workbooks.Add()
    $sheet=$book.Worksheets.Item(1)
    $P="`"$progid`""
    $cases=@(
        @{name='callback-number-vs-text';cells=@("=X5.RtdCall($P,,1)","=X5.RtdCall($P,,`"1`")")},
        @{name='callback-number-vs-typed-text';cells=@("=X5.RtdCall($P,,1)","=RTD($P,,`"1`")")},
        @{name='callback-text-vs-typed-number';cells=@("=X5.RtdCall($P,,`"1`")","=RTD($P,,1)")},
        @{name='callback-bool-vs-text';cells=@("=X5.RtdCall($P,,TRUE)","=RTD($P,,`"TRUE`")")},
        @{name='callback-float-vs-text';cells=@("=X5.RtdCall($P,,0.1+0.2)","=X5.RtdCall($P,,`"0.3`")")},
        @{name='callback-large-vs-text';cells=@("=X5.RtdCall($P,,1E+20)","=X5.RtdCall($P,,`"1E+20`")")},
        @{name='callback-text-twice';cells=@("=X5.RtdCall($P,,`"t`")","=X5.RtdCall($P,,`"t`")")},
        @{name='callback-topic-error';cells=@("=X5.RtdCall($P,,NA())")},
        @{name='callback-topic-omitted';cells=@("=X5.RtdCall($P,,)")},
        @{name='callback-progid-number';cells=@('=X5.RtdCall(1,,"e1")')},
        @{name='callback-progid-unknown';cells=@('=X5.RtdCall("No.Such.Prog.Id",,"e3")')},
        @{name='callback-topic-from-cell';cells=@('=1',"=X5.RtdCall($P,,A1)","=X5.RtdCall($P,,`"1`")")},
        @{name='callback-two-topics-vs-one';cells=@("=X5.RtdCall($P,,`"a`")","=RTD($P,,`"a`",`"b`")")}
    )
    foreach($case in $cases){
        $record=[ordered]@{case=$case.name;formulas=$case.cells}
        try{
            $before=(Lines).Count
            $sheet.Range('A1:D10').ClearContents()
            Pump 0.3
            $before=(Lines).Count
            for($i=0;$i -lt $case.cells.Count;$i++){$sheet.Cells.Item($i+1,1).Formula=$case.cells[$i]}
            Pump 1.0
            $new=@((Lines)|Select-Object -Skip $before)
            $record.connect_calls=@($new|Where-Object{$_ -like "ConnectData`t*"}).Count
            $record.rtd_rc=@($new|Where-Object{$_ -like "xlfRtdCall`t*"}|ForEach-Object{($_ -split "`t")[2]})
            $record.connect_ids=@($new|Where-Object{$_ -like "ConnectData`t*"}|ForEach-Object{($_ -split "`t")[2]})
            $record.new_log=$new
            $record.values=@(1..$case.cells.Count|ForEach-Object{[string]$sheet.Cells.Item($_,1).Text})
        }catch{$record.error=$_.ToString()}
        $records+=$record
    }
    $sheet.Range('A1:D10').ClearContents()
    Pump 0.5
    $book.Close($false)
    Pump 1.0
    $null=$OwnedExcel.ExecuteExcel4Macro('UNREGISTER("'+$xll.Replace('"','""')+'")')
    Pump 0.5
}catch{$fatal=$_.ToString()}
$value=[ordered]@{fatal=$fatal;excel_pid=$excelPid;progid=$progid;registered=$registered;cases=$records;final_log=@(Lines)}
[IO.File]::WriteAllText($Result,((ConvertTo-Json -InputObject $value -Depth 20)+"`n"),[Text.UTF8Encoding]::new($false))
