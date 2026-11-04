param([Parameter(Mandatory)][string]$SessionDirectory, [int]$LeakEvery = 50)
$ErrorActionPreference = 'Stop'
if ([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA') { throw 'Excel session host requires -STA.' }
$root = [IO.Path]::GetFullPath($SessionDirectory)
[void][IO.Directory]::CreateDirectory($root)
$stdout=[IO.StreamWriter]::new((Join-Path $root 'host.stdout'),$true,[Text.UTF8Encoding]::new($false));$stdout.AutoFlush=$true
$stderr=[IO.StreamWriter]::new((Join-Path $root 'host.stderr'),$true,[Text.UTF8Encoding]::new($false));$stderr.AutoFlush=$true
[Console]::SetOut($stdout);[Console]::SetError($stderr)
foreach ($name in 'queue','running','completed','launches') { [void][IO.Directory]::CreateDirectory((Join-Path $root $name)) }
$lock = [IO.File]::Open((Join-Path $root 'host.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
$script:hostSource=[IO.File]::ReadAllBytes($PSCommandPath)
$script:hostSha=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($script:hostSource)).ToLowerInvariant()
$captureWorker=Join-Path $PSScriptRoot '../../platform/calc-oracle-excel/worker/capture-worker.ps1'
$script:captureWorkerSha=(Get-FileHash -LiteralPath $captureWorker).Hash.ToLowerInvariant()
$script:app = $null
$script:owned = $null
$script:baseline = $null
$global:DnaExcelSessionState=[pscustomobject]@{App=$null;Baseline=$null;Root=$root;PreserveAutomationSecurity=$true}
$script:launches = 0
$script:sequence = 0
$script:ordinarySequence = 0
$script:environment = ''
$script:activeJob = $null
if (Test-Path -LiteralPath (Join-Path $root 'host.json')) {
    $previous=Get-Content -LiteralPath (Join-Path $root 'host.json') -Raw|ConvertFrom-Json
    $script:launches=[int]$previous.launches
    $script:sequence=[int]$previous.sequence
    if($previous.PSObject.Properties['ordinary_sequence']){$script:ordinarySequence=[int]$previous.ordinary_sequence}
}
$hostTicks = (Get-Process -Id $PID).StartTime.ToUniversalTime().Ticks
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class DnaSessionWindows {
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
}
public static class DnaSessionDeadline {
 static readonly object Sync = new object();
 static System.Threading.Timer timer;
 static string root, job;
 static long deadline, excelTicks;
 static int excelPid;
 public static void Start(string path) {
  root=path;
  timer=new System.Threading.Timer(Check,null,200,200);
 }
 public static void Arm(string id,int seconds) {
  lock(Sync) { job=id; deadline=DateTime.UtcNow.AddSeconds(seconds).Ticks; }
 }
 public static void Own(int pid,long ticks) { lock(Sync) { excelPid=pid; excelTicks=ticks; } }
 public static void Disarm() { lock(Sync) { job=null; } }
 static void Check(object ignored) {
  lock(Sync) {
   if(job==null || DateTime.UtcNow.Ticks<deadline) return;
   string outcome="no owned Excel";
   try {
    if(excelPid!=0 && excelPid!=3144) {
     var p=System.Diagnostics.Process.GetProcessById(excelPid);
     if(p.ProcessName.Equals("EXCEL",StringComparison.OrdinalIgnoreCase) && p.StartTime.ToUniversalTime().Ticks==excelTicks) {
      p.Kill(); outcome=p.WaitForExit(5000)?"owned Excel terminated":"owned Excel termination pending";
     } else outcome="identity changed; termination refused";
    }
   } catch(Exception e) { outcome=e.GetType().Name; }
   string id=job.Replace("\\","\\\\").Replace("\"","\\\"");
   string json="{\"job\":\""+id+"\",\"excel_pid\":"+excelPid+",\"excel_start_ticks\":"+excelTicks+",\"outcome\":\""+outcome+"\"}";
   System.IO.File.WriteAllText(System.IO.Path.Combine(root,"deadline-"+job+".json"),json);
   System.Diagnostics.Process.GetCurrentProcess().Kill();
  }
 }
 public static void Stop() { if(timer!=null) timer.Dispose(); }
}
'@
[DnaSessionDeadline]::Start($root)
function Write-Json($Path, $Value) {
    $temp = $Path + '.' + [Guid]::NewGuid().ToString('N') + '.tmp'
    [IO.File]::WriteAllText($temp, (ConvertTo-Json -InputObject $Value -Depth 40), [Text.UTF8Encoding]::new($false))
    for ($attempt=0; ; $attempt++) {
        try { [IO.File]::Move($temp, $Path, $true); break }
        catch {
            $cause=$_.Exception
            while ($cause.InnerException) { $cause=$cause.InnerException }
            if ($attempt -ge 49 -or ($cause.HResult -band 0xffff) -notin 5,32,33) { throw }
            # A reader or antivirus can briefly deny replacement on Windows. Keep the
            # original and the complete temporary JSON until the rename succeeds.
            Start-Sleep -Milliseconds 100
        }
    }
}
function Event($Kind, $Data) {
    [IO.File]::AppendAllText((Join-Path $root 'events.jsonl'), (([ordered]@{utc=[DateTime]::UtcNow.ToString('O');kind=$Kind;data=$Data}|ConvertTo-Json -Depth 30 -Compress)+"`n"))
}
function Status($Job = $script:activeJob) {
    Write-Json (Join-Path $root 'host.json') ([ordered]@{host_pid=$PID;host_start_ticks=$hostTicks;excel=$script:owned;launches=$script:launches;sequence=$script:sequence;ordinary_sequence=$script:ordinarySequence;active_job=$Job;environment=$script:environment;preserve_automation_security=$global:DnaExcelSessionState.PreserveAutomationSecurity})
}
function Read-Property($Object, [string]$Name) {
    # Resolving a COM object's runtime type repeatedly asks for its type information.
    # Use the same throwing dispatch reader as the capture worker.
    [System.__ComObject].InvokeMember($Name, [Reflection.BindingFlags]::GetProperty, $null, $Object, $null)
}
function Snapshot {
    $a = $global:DnaExcelSessionState.App
    $s = [ordered]@{}
    foreach ($key in 'Visible','DisplayAlerts','ScreenUpdating','EnableEvents','Interactive','AutomationSecurity','Calculation','Iteration','MaxIterations','MaxChange','UseSystemSeparators','DecimalSeparator','ThousandsSeparator','CalculateBeforeSave','StatusBar','CutCopyMode') {
        $value = Read-Property $a $key
        if ($key -eq 'MaxChange') { $value = [BitConverter]::DoubleToInt64Bits([double]$value).ToString('x16') }
        $s[$key] = $value
    }
    $s['mtc_enabled'] = [bool](Read-Property $a.MultiThreadedCalculation 'Enabled')
    $s['mtc_mode'] = [int](Read-Property $a.MultiThreadedCalculation 'ThreadMode')
    $s['mtc_count'] = [int](Read-Property $a.MultiThreadedCalculation 'ThreadCount')
    $s['workbooks'] = @($a.Workbooks | ForEach-Object { [string](Read-Property $_ 'FullName') })
    $s['com_addins'] = @($a.COMAddIns | ForEach-Object { ([string](Read-Property $_ 'ProgId')) + '|' + ([bool](Read-Property $_ 'Connect')) } | Sort-Object)
    $s['addins'] = @($a.AddIns2 | ForEach-Object { ([string](Read-Property $_ 'FullName')) + '|' + ([bool](Read-Property $_ 'Installed')) + '|' + ([bool](Read-Property $_ 'IsOpen')) } | Sort-Object)
    $registered = Read-Property $a 'RegisteredFunctions'
    $s['registered_functions'] = if ($null -eq $registered -or $registered -is [DBNull]) { @() } else { @($registered | ForEach-Object { [string]$_ }) }
    return $s
}
function Same($Left, $Right) { (ConvertTo-Json -InputObject $Left -Depth 30 -Compress) -ceq (ConvertTo-Json -InputObject $Right -Depth 30 -Compress) }
function Read-TraceObservations([string]$Path) {
    @(Get-Content -LiteralPath $Path -Encoding utf8|ForEach-Object { $_|ConvertFrom-Json }|Where-Object {
        $_.operation -like '=*' -or $_.operation -like 'Value2 *' -or $_.operation -like 'Application.Run *'
    }|ForEach-Object { @{operation=$_.operation;value=$_.value} })
}
function Reset-Baseline {
    $a = $global:DnaExcelSessionState.App
    $baseline=$global:DnaExcelSessionState.Baseline
    if ($baseline -and $global:DnaExcelSessionState.PreserveAutomationSecurity -and
        (Read-Property $a 'AutomationSecurity') -ne $baseline['AutomationSecurity']) {
        throw 'AutomationSecurity drift; this session forbids writing that setting.'
    }
    $a.EnableEvents = $false
    $a.DisplayAlerts = $false
    $a.CutCopyMode = 0
    while ($a.Workbooks.Count -gt 0) { $a.Workbooks.Item(1).Close($false) }
    # Excel returns CVErr for calculation properties with no books. Record a real blank book.
    $sentinel = $a.Workbooks.Add()
    $a.Calculation = -4135
    $a.Iteration = $false
    $a.MaxIterations = 100
    $a.MaxChange = 0.001
    $a.CalculateBeforeSave = $false
    if ($baseline) {
        foreach ($key in 'Visible','DisplayAlerts','ScreenUpdating','EnableEvents','Interactive','UseSystemSeparators','DecimalSeparator','ThousandsSeparator','StatusBar') { $a.$key = $baseline[$key] }
        $a.MultiThreadedCalculation.Enabled = $baseline['mtc_enabled']
        $a.MultiThreadedCalculation.ThreadMode = $baseline['mtc_mode']
        if ($baseline['mtc_mode'] -eq 1) { $a.MultiThreadedCalculation.ThreadCount = $baseline['mtc_count'] }
    } else {
        $a.Visible=$false; $a.ScreenUpdating=$false; $a.Interactive=$false; $a.StatusBar=$false
    }
    $sentinel.SaveAs((Join-Path $global:DnaExcelSessionState.Root 'baseline.xlsx'),51)
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    Snapshot
}
function Restore-JobBaseline {
    # A condition may contain multiple independently seeded workbook trials.
    $state=Reset-Baseline
    if(-not (Same $state $global:DnaExcelSessionState.Baseline)){throw 'Baseline drift between trials; condition refused.'}
    return $state
}
function Start-JobTrial {
    $state=Snapshot
    if(-not(Same $state $global:DnaExcelSessionState.Baseline)){throw 'Baseline drift before a workbook trial; condition refused.'}
    $global:DnaExcelSessionState.App.Workbooks.Item(1).Close($false)
    return $state
}
function Stop-Owned([string]$Reason) {
    if (-not $script:owned) { return }
    $identity=$script:owned
    Event 'stop' @{reason=$Reason;excel=$identity}
    try { $script:app.DisplayAlerts=$false; $script:app.EnableEvents=$false; $script:app.Quit() } catch { Event 'quit-error' $_.Exception.Message }
    $script:app=$null
    $global:DnaExcelSessionState.App=$null
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    $p=Get-Process -Id $identity.pid -ErrorAction SilentlyContinue
    if ($p -and -not $p.WaitForExit(5000)) {
        if ($p.Id -eq 3144 -or $p.StartTime.ToUniversalTime().Ticks -ne $identity.start_ticks -or $p.ProcessName -ine 'EXCEL') { throw 'Owned Excel identity changed; refusing termination.' }
        Stop-Process -Id $p.Id
        [void]$p.WaitForExit(5000)
    }
    $script:owned=$null; $script:baseline=$null
    $global:DnaExcelSessionState.Baseline=$null
    [DnaSessionDeadline]::Own(0,0)
    Status
}
function Start-Owned([string]$Reason) {
    $before=@(Get-Process EXCEL -ErrorAction SilentlyContinue | ForEach-Object Id)
    $script:launches++
    Event 'launch-attempt' @{launch=$script:launches;reason=$Reason;preexisting_pids=$before}
    Status
    $a=New-Object -ComObject Excel.Application
    [uint32]$newPid=0
    [void][DnaSessionWindows]::GetWindowThreadProcessId([IntPtr]$a.Hwnd,[ref]$newPid)
    $process=Get-Process -Id $newPid
    $command=(Get-CimInstance Win32_Process -Filter "ProcessId=$newPid").CommandLine
    if (-not $newPid -or $newPid -eq 3144 -or $newPid -in $before -or $process.ProcessName -ine 'EXCEL' -or $command -match '(?i)/restore') { throw "Excel activation is not owned ($newPid); it will not be quit or terminated." }
    $script:app=$a
    $global:DnaExcelSessionState.App=$a
    $script:owned=[ordered]@{pid=[int]$newPid;start_ticks=$process.StartTime.ToUniversalTime().Ticks;command_line=$command;launch=$script:launches}
    [DnaSessionDeadline]::Own([int]$newPid,$script:owned.start_ticks)
    # Publish ownership before any further COM calls, including baseline setup.
    Status
    $script:baseline=Reset-Baseline
    $global:DnaExcelSessionState.Baseline=$script:baseline
    $sourceName=$script:launches.ToString()+'-host.ps1'
    [IO.File]::WriteAllBytes((Join-Path $root ('launches/'+$sourceName)),$script:hostSource)
    Write-Json (Join-Path $root ('launches/'+$script:launches+'.json')) @{excel=$script:owned;baseline=$script:baseline;reason=$Reason;version=[string]$a.Version;build=[string]$a.Build;host_script=$sourceName;host_script_sha256=$script:hostSha}
    Event 'launched' $script:owned
    Status
}
function Run-Script($Job, [string]$Directory, [switch]$Repeat) {
    # Jobs retain their existing first-workbook semantics; the baseline book contains no data.
    $script:app.Workbooks.Item(1).Close($false)
    $parameters=@{}
    foreach ($p in $Job.parameters.PSObject.Properties) { $parameters[$p.Name]=$p.Value }
    if ($Repeat) {
        if ($parameters.ContainsKey('Request') -and $parameters.ContainsKey('Result')) {
            $request=Get-Content -LiteralPath $parameters.Request -Raw -Encoding utf8|ConvertFrom-Json
            if ($request.PSObject.Properties['action'] -and $request.action -eq 'prepare') { throw 'Fixture preparation is not a replayable observation job; no leakage pass is recorded.' }
            if ($request.PSObject.Properties['output']) { $request.output=Join-Path $Directory ([IO.Path]::GetFileName($request.output)) }
            $parameters.Request=Join-Path $Directory 'request.json'
            $parameters.Result=Join-Path $Directory 'result.json'
            Write-Json $parameters.Request $request
        } elseif ($parameters.ContainsKey('RunDirectory')) { $parameters.RunDirectory=Join-Path $Directory 'trace' }
        else { throw 'Leakage sampling needs Request/Result or RunDirectory parameters.' }
    }
    $parameters['OwnedExcel']=$script:app
    $executionScript=$Job.script
    $executionHash=(Get-FileHash -LiteralPath $Job.script).Hash.ToLowerInvariant()
    if ([IO.Path]::GetFileName($Job.script) -ceq 'capture-worker.ps1' -and $executionHash -ceq $script:captureWorkerSha) {
        # This worker uses absolute request paths and no script-relative resources.
        # One immutable path avoids retaining another parsed copy for every workbook.
        $cache=Join-Path $root ('worker-scripts/'+$executionHash)
        [void][IO.Directory]::CreateDirectory($cache)
        $executionScript=Join-Path $cache 'capture-worker.ps1'
        if (-not(Test-Path -LiteralPath $executionScript)) { Copy-Item -LiteralPath $Job.script -Destination $executionScript }
        if ((Get-FileHash -LiteralPath $executionScript).Hash.ToLowerInvariant() -cne $executionHash) {
            throw 'Cached capture worker differs from the submitted script; execution refused.'
        }
    }
    Push-Location $Job.working_directory
    try { & $executionScript @parameters *> (Join-Path $Directory 'worker.log') } finally { Pop-Location }
    if ($parameters.ContainsKey('Result')) {
        $result=Get-Content -LiteralPath $parameters.Result -Raw -Encoding utf8|ConvertFrom-Json
        if ($result.PSObject.Properties['fatal'] -and $result.fatal) { throw $result.fatal }
        return @{result=$parameters.Result;data=$result;eligible=($null -ne $result.PSObject.Properties['rows'] -or $null -ne $result.PSObject.Properties['cells']);execution_script=$executionScript;execution_script_sha256=$executionHash}
    }
    if ($parameters.ContainsKey('RunDirectory')) {
        $controller=Join-Path $parameters.RunDirectory 'controller.jsonl'
        $values=@(Read-TraceObservations $controller)
        return @{result=$controller;data=@{trace_values=$values};eligible=($values.Count -gt 0);execution_script=$executionScript;execution_script_sha256=$executionHash}
    }
    return @{result=$null;data=$null;eligible=$false;execution_script=$executionScript;execution_script_sha256=$executionHash}
}
function Comparable($Run) {
    if (-not $Run.data) { return $null }
    if ($Run.data -is [System.Collections.IDictionary] -and $Run.data.Contains('trace_values')) { return $Run.data.trace_values }
    if ($Run.data.PSObject.Properties['rows']) { return $Run.data.rows }
    if (-not $Run.data.PSObject.Properties['cells']) { throw 'This job has no registered observation comparison; a leakage pass cannot be inferred.' }
    return @($Run.data.cells,$Run.data.reopened,$Run.data.saved)
}
if (Test-Path -LiteralPath (Join-Path $root 'running')) {
    foreach($interrupted in Get-ChildItem -LiteralPath (Join-Path $root 'running') -Filter '*.json') {
        $oldJob=Get-Content -LiteralPath $interrupted.FullName -Raw|ConvertFrom-Json
        $oldDirectory=Join-Path $root ('completed/'+$oldJob.id)
        $oldReceipt=Join-Path $oldDirectory 'receipt.json'
        $draft=Join-Path $oldDirectory 'receipt-start.json'
        if (-not (Test-Path -LiteralPath $oldReceipt) -and (Test-Path -LiteralPath $draft)) {
            $record=Get-Content -LiteralPath $draft -Raw|ConvertFrom-Json -AsHashtable
            $record.success=$false;$record.error='Previous session host exited before completing this job; observations remain unqualified.'
            $record['finished_utc']=[DateTime]::UtcNow.ToString('O');$record['launches_after']=$script:launches
            $record['interrupted']=$true
            Write-Json $oldReceipt $record
        }
    }
}
Status
Event 'host-start' @{pid=$PID;start_ticks=$hostTicks;leak_every=$LeakEvery;script_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash}
try {
    while (-not (Test-Path -LiteralPath (Join-Path $root 'shutdown.request'))) {
        $candidates=@(Get-ChildItem -LiteralPath (Join-Path $root 'queue') -Filter '*.json' | ForEach-Object {
            $candidateFile=$_
            try {
                $j=Get-Content -LiteralPath $candidateFile.FullName -Raw -Encoding utf8 -ErrorAction Stop|ConvertFrom-Json
                [pscustomobject]@{file=$candidateFile;job=$j;fresh=[bool]$j.fresh_required;group=([string]$j.environment -cne $script:environment)}
            }catch{if(Test-Path -LiteralPath $candidateFile.FullName){throw}}
        } | Sort-Object fresh,group,@{Expression={$_.job.environment}},@{Expression={$_.file.Name}})
        if (-not $candidates.Count) { Start-Sleep -Milliseconds 100; continue }
        $candidate=$candidates[0]; $job=$candidate.job
        if ($job.id -notmatch '^[A-Za-z0-9_-]+$' -or [int]$job.timeout_seconds -lt 1) { throw 'Invalid job ID or deadline.' }
        $destination=Join-Path $root ('running/'+$candidate.file.Name)
        try{Move-Item -LiteralPath $candidate.file.FullName -Destination $destination -ErrorAction Stop}
        catch{if(-not(Test-Path -LiteralPath $candidate.file.FullName)){continue};throw}
        $directory=Join-Path $root ('completed/'+$job.id)
        [void][IO.Directory]::CreateDirectory($directory)
        $script:sequence++
        $script:activeJob=$job.id
        [DnaSessionDeadline]::Arm($job.id,[int]$job.timeout_seconds)
        Status
        $receipt=[ordered]@{format='dnacalc-excel-session-job/1';id=$job.id;sequence=$script:sequence;environment=$job.environment;fresh_required=[bool]$job.fresh_required;host_pid=$PID;started_utc=[DateTime]::UtcNow.ToString('O');launches_before=$script:launches;success=$false;error=$null;baseline_before=$null;baseline_after=$null;leakage=$null}
        $receipt['script']=$job.script
        $receipt['host_script_sha256']=$script:hostSha
        $receipt['preserve_automation_security']=$global:DnaExcelSessionState.PreserveAutomationSecurity
        $receipt['script_sha256']=(Get-FileHash -LiteralPath $job.script).Hash.ToLowerInvariant()
        $receipt['job']=$job
        Copy-Item -LiteralPath $destination -Destination (Join-Path $directory 'job.json')
        Write-Json (Join-Path $directory 'receipt-start.json') $receipt
        try {
            if ($job.fresh_required -and $script:app) { Stop-Owned 'fresh-required' }
            if (-not $script:app) { Start-Owned $(if($job.fresh_required){'fresh-required'}else{'first job or recovery'}) }
            try { $current=Snapshot; $same=Same $current $script:baseline } catch { $same=$false; Event 'unreachable' $_.Exception.Message }
            if (-not $same) { Event 'idle-drift' @{observed=$current;expected=$script:baseline}; Stop-Owned 'crash or idle drift'; Start-Owned 'crash or idle drift' }
            $receipt.baseline_before=Snapshot
            $receipt['baseline']= $script:baseline
            $receipt['excel']=$script:owned
            $script:environment=$job.environment
            Status $job.id
            $run=Run-Script $job $directory
            $receipt['execution_script']=$run.execution_script
            $receipt['execution_script_sha256']=$run.execution_script_sha256
            if ($run.result) { $receipt['result_sha256']=(Get-FileHash -LiteralPath $run.result).Hash.ToLowerInvariant() }
            $receipt['observed_after_job']=Snapshot
            $receipt.baseline_after=Reset-Baseline
            if (-not (Same $receipt.baseline_after $script:baseline)) { throw 'Baseline drift after reset; outputs retained, job refused.' }
            $receipt.success=$true
            $receipt['sampling_eligible']=($run.eligible -and -not $job.fresh_required)
            if($receipt.sampling_eligible){$script:ordinarySequence++}
            $receipt['ordinary_sequence']=$script:ordinarySequence
            if ($receipt.sampling_eligible -and $script:ordinarySequence % $LeakEvery -eq 0) {
                # This repeat is itself fresh-required. Keep its process for subsequent jobs.
                Stop-Owned 'fresh-required leakage sample'
                $freshDirectory=Join-Path $directory 'fresh-repeat'; [void][IO.Directory]::CreateDirectory($freshDirectory)
                try {
                    Start-Owned 'one-in-50 leakage sample'
                    $fresh=Run-Script $job $freshDirectory -Repeat
                    $after=Reset-Baseline
                    $receipt.leakage=@{sampled=$true;excel=$script:owned;comparable=($null -ne $run.data);equal=$(if($run.data){Same (Comparable $run) (Comparable $fresh)}else{$null});baseline_verified=(Same $after $script:baseline);result=$fresh.result}
                } catch { $receipt.leakage=@{sampled=$true;error=$_.Exception.ToString();equal=$false} }
                finally { Status $job.id }
            }
        } catch {
            $receipt.success=$false
            $receipt.error=$_.Exception.ToString()
            Event 'job-error' @{id=$job.id;error=$receipt.error}
            try {
                $reset=Reset-Baseline
                if (-not (Same $reset $script:baseline)) { Stop-Owned 'baseline drift after failed job' }
            } catch { Stop-Owned 'crash or unreachable baseline'; Event 'cleanup-error' $_.Exception.Message }
        } finally {
            $receipt['finished_utc']=[DateTime]::UtcNow.ToString('O')
            $receipt['launches_after']=$script:launches
            Write-Json (Join-Path $directory 'receipt.json') $receipt
            [DnaSessionDeadline]::Disarm()
            Event 'job-finished' $receipt
            $script:activeJob=$null
            Status
        }
    }
} finally { [DnaSessionDeadline]::Disarm(); Stop-Owned 'host shutdown'; Event 'host-stop' $PID; [DnaSessionDeadline]::Stop(); $lock.Dispose() }
