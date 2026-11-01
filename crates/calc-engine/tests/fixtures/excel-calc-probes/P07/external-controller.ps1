param([Parameter(Mandatory)][string]$ProbeDirectory,[Parameter(Mandatory)][string]$RunId,[string]$OxVbaRoot='C:/Work/DnaCalc/OxVba')
$ErrorActionPreference='Stop'
$log=Join-Path $ProbeDirectory ("external-"+$RunId+'.jsonl')
$stop=Join-Path $ProbeDirectory ("external-"+$RunId+'.stop')
$ledger=Join-Path $ProbeDirectory ("raw/"+$RunId+'/owned-processes.jsonl')
# Use the existing supervisor's exact-PID native-object attachment implementation.
$worker=Get-Content -Raw -LiteralPath (Join-Path $OxVbaRoot 'scripts/excel-vba-oracle-worker.ps1')
$native=[regex]::Match($worker,"(?s)Add-Type @'\r?\n(.*?)\r?\n'@")
if(-not $native.Success){throw 'Existing native attachment code absent'}
Add-Type $native.Groups[1].Value
Add-Type @'
using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
public static class ProbeLatchController {
 public static Task<long> WaitEntry(int pid,long latch) {
  return Task.Run(()=> {
   string name="Local\\DnaCalcProbeEntered-"+pid+"-"+Math.Abs(latch);
   for(int i=0;i<1000;i++) {
    try {using(var e=EventWaitHandle.OpenExisting(name)) {if(e.WaitOne(5000)) return Environment.TickCount64;return -1;}}
    catch(WaitHandleCannotBeOpenedException){Thread.Sleep(10);}
   }
   return -1;
  });
 }
 public static Task<string[]> Release(int pid,long first,long second,int firstDelay,int secondDelay) {
  return Task.Run(()=> {
   var logs=new List<string>(); int previous=0;
   foreach(var pair in new[]{Tuple.Create(first,firstDelay),Tuple.Create(second,secondDelay)}) {
    if(pair.Item1==0) continue;
    Thread.Sleep(Math.Max(0,pair.Item2-previous)); previous=pair.Item2;
    string name="Local\\DnaCalcProbe-"+pid+"-"+Math.Abs(pair.Item1);
    bool opened=false;
    for(int i=0;i<100;i++) {
     try { using(var e=EventWaitHandle.OpenExisting(name)) {e.Set();opened=true;} break; }
     catch(WaitHandleCannotBeOpenedException){Thread.Sleep(10);}
    }
    logs.Add(name+";opened="+opened+";tick="+Environment.TickCount64+";utc="+DateTime.UtcNow.ToString("o"));
   }
   return logs.ToArray();
  });
 }
}
'@
function Record($value){$value|ConvertTo-Json -Depth 8 -Compress|Add-Content -LiteralPath $log -Encoding utf8NoBOM}
$seen=[Collections.Generic.HashSet[string]]::new()
$cachedApp=$null
$cachedPid=0
$deadline=[DateTime]::UtcNow.AddSeconds(1500)
Record @{event='started';helper_pid=$PID;utc=[DateTime]::UtcNow.ToString('o')}
while(-not(Test-Path -LiteralPath $stop) -and [DateTime]::UtcNow -lt $deadline){
 foreach($file in @(Get-ChildItem -LiteralPath $ProbeDirectory -Filter ("external-control-"+$RunId+'-*.txt'))){
  if(-not $seen.Add($file.FullName)){continue}
  $app=$null;$release=$null;$done=$file.FullName+'.done'
  try{
   $p=(Get-Content -Raw -LiteralPath $file.FullName).TrimEnd("`r","`n").Split("`t")
   $excelPid=[int]$p[0];$bookName=$p[1];$operation=$p[2];$oldLatch=[long]$p[3];$newLatch=[long]$p[4]
   $targetPath=$p[5];$controllerBook=$p[6]
   if($excelPid -eq 3144){throw 'Excluded owner PID'}
   $owned=@(Get-Content -LiteralPath $ledger|ForEach-Object{$_|ConvertFrom-Json -DateKind String}|Where-Object{[int]$_.pid -eq $excelPid})
   if($owned.Count -ne 1){throw 'Target is not the exact supervisor-owned Excel process'}
   $process=Get-Process -Id $excelPid
   if($process.StartTime.ToUniversalTime().ToString('o') -ne [string]$owned[0].process_start_utc){throw 'Owned process start differs'}
   if($cachedApp -and $cachedPid -eq $excelPid){$app=$cachedApp}
   else{
   $windows=[ExcelOracleNativeMethods]::EnumerateOwnedWindows([uint32]$excelPid)
   if(-not $windows.Succeeded -or $windows.Truncated){throw 'Exact-PID enumeration incomplete'}
   foreach($window in @($windows.Windows|Where-Object ClassName -eq 'EXCEL7')){
    $object=$null
    $hr=[ExcelOracleNativeMethods]::TryGetNativeObjectFromWindow([IntPtr]$window.Hwnd,[ref]$object)
    if($hr -eq 0 -and $null -ne $object){$app=$object.Application;break}
   }
   if($null -eq $app){throw 'Owned Excel native object unavailable'}
   }
   [uint32]$actualPid=0
   [void][ExcelOracleNativeMethods]::GetWindowThreadProcessId([IntPtr]$app.Hwnd,[ref]$actualPid)
   if($actualPid -ne $excelPid){throw 'Attached application differs from owned PID'}
   if($operation -eq 'attach-only'){
      $cachedApp=$app;$cachedPid=$excelPid
      Record @{event='attached';excel_pid=$excelPid;tick=[Environment]::TickCount64}
      [IO.File]::WriteAllText((Join-Path $ProbeDirectory ("external-"+$RunId+'.attached')),'attached')
      continue
   }
   if($cachedApp){
      $wb=$app.Workbooks.Item($bookName);$ws=$wb.Worksheets.Item(1)
      $cellA=$ws.Range('A1');$cellB=$ws.Range('B1');$cellC=$ws.Range('C1');$cellF=$ws.Range('F1')
      $preflight=@($cellA.Value2,$cellB.Value2,$cellC.Value2)
      Record @{event='preflight';operation=$operation;excel_pid=$excelPid;values=$preflight;tick=[Environment]::TickCount64}
      [IO.File]::WriteAllText($file.FullName+'.ready','preflight-complete')
      $enteredTick=[ProbeLatchController]::WaitEntry($excelPid,$oldLatch).GetAwaiter().GetResult()
      if($enteredTick -lt 0){throw 'Expected native async entry was not observed'}
      Record @{event='native-entry-observed';operation=$operation;excel_pid=$excelPid;tick=$enteredTick;controller_resume_tick=[Environment]::TickCount64}
   }
   # This thread releases the latch even if Excel rejects or blocks COM access.
   if($operation -eq 'reverse-release'){$release=[ProbeLatchController]::Release($excelPid,$newLatch,$oldLatch,400,900)}
   else{$release=[ProbeLatchController]::Release($excelPid,$oldLatch,$newLatch,900,950)}
   $start=[Environment]::TickCount64;$attempt=0;$success=$false;$failure=$null;$partial=$false
   while([Environment]::TickCount64-$start -lt 750 -and -not $success){
    $attempt++
    try{
     if(-not $cachedApp){$wb=$app.Workbooks.Item($bookName);$ws=$wb.Worksheets.Item(1);$cellA=$ws.Range('A1');$cellB=$ws.Range('B1');$cellC=$ws.Range('C1');$cellF=$ws.Range('F1')}
     $before='not-read-during-pending'
     Record @{event='operation-started';operation=$operation;excel_pid=$excelPid;attempt=$attempt;tick=[Environment]::TickCount64;calculation_state_before=$before}
     switch($operation){
      'normal' {}
      'overwrite' {$cellC.Value2=999}
      'undo' {$cellC.Value2=999;$partial=$true;Record @{event='partial-step';operation=$operation;step='overwrite999-returned';tick=[Environment]::TickCount64};$app.Undo()}
      'arg-edit' {$cellA.Value2=99}
      'move' {$cellC.Cut($cellF)}
      'close-reopen' {$wb.Close($false);$wb=$app.Workbooks.Open($targetPath,0,$false)}
      'supersede' {$cellA.Value2=99;$cellC.Calculate()}
      'reverse-release' {$cellA.Value2=99;$cellB.Value2=[double]$newLatch;$cellC.Calculate()}
      'reload' {$value=$app.Run("'"+$controllerBook+"'!OracleProbe.ReloadAsync");Record @{event='reload-command-result';detail=$value;tick=[Environment]::TickCount64}}
      'save-pending' {$app.CalculateBeforeSave=$false;$wb.SaveCopyAs($targetPath.Replace('.xlsx','-pending.xlsx'))}
      default {throw "Unimplemented operation $operation"}
     }
     $success=$true
     Record @{event='operation-returned';operation=$operation;excel_pid=$excelPid;attempt=$attempt;calculation_state_before=$before;tick=[Environment]::TickCount64;start_tick=$start;utc=[DateTime]::UtcNow.ToString('o')}
    }catch{
      $failure=$_.Exception.Message
      $busy=$failure -match 'rejected by callee|RPC_E_CALL_REJECTED|RPC_E_SERVERCALL_RETRYLATER|0x80010001|0x8001010A'
      if($partial -or -not $busy){break}
      Start-Sleep -Milliseconds 50
    }
   }
   if(-not $success){Record @{event='operation-unavailable';operation=$operation;failure=$failure;attempts=$attempt;partial_step_completed=$partial;may_have_mutated=$true;tick=[Environment]::TickCount64}}
   foreach($entry in $release.GetAwaiter().GetResult()){Record @{event='latch-release';operation=$operation;detail=$entry}}
  }catch{Record @{event='controller-fault';file=$file.Name;message=$_.Exception.Message;tick=[Environment]::TickCount64}}
  finally{
   [IO.File]::WriteAllText($done,'controller-complete')
   if($app -and -not [object]::ReferenceEquals($app,$cachedApp)){[void][Runtime.InteropServices.Marshal]::ReleaseComObject($app)}
  }
 }
 Start-Sleep -Milliseconds 20
}
Record @{event='stopped';helper_pid=$PID;utc=[DateTime]::UtcNow.ToString('o')}
if($cachedApp){[void][Runtime.InteropServices.Marshal]::ReleaseComObject($cachedApp)}
