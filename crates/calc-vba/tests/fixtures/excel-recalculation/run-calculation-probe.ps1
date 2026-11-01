param(
    [Parameter(Mandatory)][string]$ProbeDirectory,
    [string]$OxVbaRoot = 'C:/Work/DnaCalc/OxVba',
    [int]$Repetitions = 10,
    [string]$Cases = '',
    [switch]$Reopen,
    [string]$ArtifactPrefix = '',
    [string]$NativePath = '',
    [switch]$ExternalController,
    [string]$SourceFile = '',
    [switch]$ExternalDirect,
    [switch]$UIPause,
    [string]$RunId = ('calc-probe-' + [Guid]::NewGuid().ToString('N')),
    [switch]$SafeMode
)
$ErrorActionPreference = 'Stop'
$probe = (Resolve-Path -LiteralPath $ProbeDirectory).Path
$common = Join-Path $PSScriptRoot 'calculation-common.bas'
$selectedSource = if($Reopen){Join-Path $PSScriptRoot 'reopen-calculation-probe.bas'}elseif($SourceFile){$SourceFile}else{Join-Path $probe 'probe.bas'}
if($Reopen -and -not $Cases){throw '-Reopen requires explicit mode~filename cases'}
$probeSource = Get-Content -LiteralPath $selectedSource -Raw
$firstProcedure = [regex]::Match($probeSource, '(?m)^(?:Private|Public)\s+(?:Sub|Function)\b')
if (-not $firstProcedure.Success) {throw 'Probe has no procedures'}
$declarations = $probeSource.Substring(0,$firstProcedure.Index)
$procedures = $probeSource.Substring($firstProcedure.Index)
$source = (Get-Content -LiteralPath $common -Raw).Replace('Public Sub Note(', $declarations + "`nPublic Sub Note(") + "`n" + $procedures
$source = $source.Replace('__OUTPUT__', $probe.Replace('"','""')).Replace('__REPETITIONS__', [string]$Repetitions)
$caseStatements='SelectedCaseIds = ""'
for($offset=0;$offset -lt $Cases.Length;$offset+=300){
    $chunk=$Cases.Substring($offset,[Math]::Min(300,$Cases.Length-$offset)).Replace('"','""')
    $caseStatements += "`r`n    SelectedCaseIds = SelectedCaseIds & " + '"' + $chunk + '"'
}
$source = $source.Replace('SelectedCaseIds = "__CASES__"', $caseStatements)
$source = $source.Replace('__ARTIFACT_PREFIX__', $ArtifactPrefix.Replace('"','""'))
$source = $source.Replace('__RUN_ID__', $RunId.Replace('"','""'))
$source = $source.Replace('__EXTERNAL_DIRECT__', $(if($ExternalDirect){'1'}else{'0'}))
if($ExternalDirect -and (-not $ExternalController -or -not $Cases)){throw '-ExternalDirect requires -ExternalController and explicit -Cases'}
$native = if($NativePath){(Resolve-Path -LiteralPath $NativePath).Path}else{Join-Path (Split-Path $probe -Parent) 'P03/native-probe.xll'}
$source = $source.Replace('__NATIVE__', $native.Replace('"','""'))
$module = Join-Path $probe 'injected.bas'
[IO.File]::WriteAllText($module, $source)
$environment = [ordered]@{
    captured_utc = [DateTime]::UtcNow.ToString('o')
    os = [Environment]::OSVersion.VersionString
    process_bitness = [IntPtr]::Size * 8
    cpu = @((Get-CimInstance Win32_Processor) | Select-Object Name, Manufacturer, NumberOfCores, NumberOfLogicalProcessors)
    locale = (Get-Culture).Name
    supervisor = 'OxVba/scripts/run-excel-vba-oracle.ps1, selected windows-scalar-interop descriptor'
    supervisor_commit = (& git -C $OxVbaRoot rev-parse HEAD)
    fixture_ancestor = 'crates/calc-vba/tests/fixtures/excel-recalculation/probe.bas'
    safe_mode_requested = [bool]$SafeMode
    native_image = $native
    native_sha256 = if(Test-Path -LiteralPath $native){(Get-FileHash -LiteralPath $native -Algorithm SHA256).Hash}else{$null}
    excel_file_version = (Get-Item -LiteralPath 'C:/Program Files/Microsoft Office/root/Office16/EXCEL.EXE').VersionInfo.FileVersion
}
$environment | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $probe 'machine.json') -Encoding utf8NoBOM
# Extend the existing owned-PID VBE supervisor. Its worker, containment, compile
# verification, dialog observation, and cleanup are used without replacement.
$supervisorPath = Join-Path $OxVbaRoot 'scripts/run-excel-vba-oracle.ps1'
$supervisor = Get-Content -Raw -LiteralPath $supervisorPath
$supervisor = $supervisor.Replace('$PSScriptRoot', "'" + (Join-Path $OxVbaRoot 'scripts') + "'")
$anchor = '$repoRoot = '
$at = $supervisor.IndexOf($anchor)
if ($at -lt 0) { throw 'Supervisor insertion point absent' }
$extension = @'
$script:calculationProbeCase = @(Get-ExcelOracleHarnessCases | Where-Object id -eq 'windows-scalar-interop')[0]
$script:calculationProbeCase.module_source = Get-Content -Raw -LiteralPath '__MODULE__'
$script:calculationProbeCase.module_name = 'OracleProbe'
$script:calculationProbeCase.run_procedure = 'OracleProbe.RunProbe'
$script:calculationProbeCase.expected_value = 'OBSERVATION_ONLY'
$script:calculationProbeCase.purpose = 'Excel calculation measurements; expected-value mismatch is not a behavioral verdict'
function Get-ExcelOracleHarnessCases { return @($script:calculationProbeCase) }

'@
$extension = $extension.Replace('__MODULE__', $module.Replace("'", "''"))
$supervisor = $supervisor.Insert($at, $extension)
$workerSource = Get-Content -Raw -LiteralPath (Join-Path $OxVbaRoot 'scripts/excel-vba-oracle-worker.ps1')
$workerSource = $workerSource.Replace('$PSScriptRoot', "'" + (Join-Path $OxVbaRoot 'scripts') + "'")
$workerSource = $workerSource.Replace('[ValidateRange(5, 600)][int]$CaseTimeoutSeconds', '[ValidateRange(5, 1500)][int]$CaseTimeoutSeconds')
$workerSource = $workerSource.Replace('        $errorMessage = $_.Exception.Message', '        $errorMessage = $_.Exception.Message' + "`n" + '        ($_.Exception.ToString() + "`n" + $_.ScriptStackTrace) | Set-Content -LiteralPath (Join-Path $caseDirectory "measurement-error.txt") -Encoding utf8NoBOM')
$startupAnchor = '            $blocked = @($blockingWindows'
$startupAt = $workerSource.IndexOf($startupAnchor)
if ($startupAt -lt 0) { throw 'Owned startup observation insertion point absent' }
$startupExtension = @'
            if ($SafeMode -and -not $startupNoticeAcknowledged -and $blockingWindows.Count -eq 1 -and
                [string]$blockingWindows[0].ClassName -ceq '#32770' -and
                @($startupText | Where-Object { $_ -like "Excel is running into problems with the '*testcrashguard-addin64.xll' add-in.*Do you want to disable it now?" }).Count -eq 1) {
                $notice = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr][int64]$blockingWindows[0].Hwnd)
                if ($notice.Current.ProcessId -ne $process.Id) { throw 'Startup prompt process identity changed' }
                $button = $notice.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
                    [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'No'))
                if ($null -eq $button -or $button.Current.ProcessId -ne $process.Id) { throw 'Owned startup prompt No button absent' }
                ([System.Windows.Automation.InvokePattern]$button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
                [ordered]@{pid=$process.Id; action='decline-disable-TestCrashGuard'; utc=[DateTime]::UtcNow.ToString('o')} |
                    ConvertTo-Json | Set-Content -LiteralPath "$DiagnosticPath.startup-action.json" -Encoding utf8NoBOM
                $startupNoticeAcknowledged = $true
                continue
            }
'@
$workerSource = $workerSource.Insert($startupAt, $startupExtension + "`n")
if($ExternalDirect){
$directHook=@'
        $attachControl='__PROBE__\external-control-__RUN__-attach.txt'
        [IO.File]::WriteAllText($attachControl,([string]$excelPid+"`t"+$workbook.Name+"`tattach-only`t0`t0`t`t"+$workbook.Name))
        $attachDeadline=[DateTime]::UtcNow.AddSeconds(45)
        while(-not(Test-Path -LiteralPath '__PROBE__\external-__RUN__.attached') -and [DateTime]::UtcNow -lt $attachDeadline){Start-Sleep -Milliseconds 20}
        if(-not(Test-Path -LiteralPath '__PROBE__\external-__RUN__.attached')){throw 'External controller did not attach before direct calculation'}
        for($rep=1;$rep -le __REPS__;$rep++){
            foreach($condition in '__CASES_DIRECT__'.Split('|')){
                $caseId=$condition+'/'+$rep
                $artifact='__PROBE__\__PREFIX__'+$condition+'-'+$rep.ToString('00')+'.xlsm'
                $created=[string]$excel.Run("'"+$workbook.Name+"'!OracleProbe.CreateOneCase",$caseId,$artifact)
                $createdParts=$created.Split("`n",2)
                $cloneName=$createdParts[0]
                if($createdParts.Count -gt 1){$runValue += "`n"+$createdParts[1]}
                $controlPath=[string]$excel.Run("'"+$cloneName+"'!OracleProbe.DirectControlPath")
                $preflightDeadline=[DateTime]::UtcNow.AddSeconds(30)
                while(-not(Test-Path -LiteralPath ($controlPath+'.ready')) -and [DateTime]::UtcNow -lt $preflightDeadline){Start-Sleep -Milliseconds 20}
                if(-not(Test-Path -LiteralPath ($controlPath+'.ready'))){throw 'External control preflight did not finish before calculation'}
                [void]$excel.Run("'"+$cloneName+"'!OracleProbe.BeginDirectCase")
                $started=[Environment]::TickCount64
                try{
                    $excel.Calculate()
                    $runValue += "`n"+$caseId+"`tdirect-com`tmethod-return`tduration-ms="+([Environment]::TickCount64-$started)
                }catch{
                    $runValue += "`n"+$caseId+"`tdirect-com`tmethod-error`t"+$_.Exception.Message
                }
                $runValue += "`n"+[string]$excel.Run("'"+$cloneName+"'!OracleProbe.CompleteDirectCase")
                $excel.Workbooks.Item($cloneName).Close($false)
            }
        }

'@
$directHook=$directHook.Replace('__PROBE__',$probe.Replace("'","''")).Replace('__RUN__',$RunId.Replace("'","''")).Replace('__REPS__',[string]$Repetitions).Replace('__CASES_DIRECT__',$Cases.Replace("'","''")).Replace('__PREFIX__',$ArtifactPrefix.Replace("'","''"))
$hookAt=$workerSource.IndexOf('        $guardianHealthy = $guardian -and -not $guardian.HasExited')
if($hookAt -lt 0){throw 'Direct command insertion point absent'}
$workerSource=$workerSource.Insert($hookAt,$directHook)
}
if($UIPause){
$uiHook=@'
        $uiRoot='__PROBE__'
        $uiRun='__RUN__'
        $uiOwner=$excel.Workbooks.Item('P07-ui-owned.xlsm')
        @{pid=$excelPid;hwnd=[string]$excel.Hwnd;workbook=$uiOwner.Name}|ConvertTo-Json|Set-Content (Join-Path $uiRoot ('ui-ready-'+$uiRun+'.json'))
        $uiLast=0
        $uiDeadline=[DateTime]::UtcNow.AddSeconds(1500)
        while(-not(Test-Path (Join-Path $uiRoot ('ui-stop-'+$uiRun))) -and [DateTime]::UtcNow -lt $uiDeadline){
            $commandPath=Join-Path $uiRoot ('ui-command-'+$uiRun+'.json')
            if(Test-Path $commandPath){
                try {$command=Get-Content $commandPath -Raw|ConvertFrom-Json} catch [System.IO.IOException] {continue}
                if([int]$command.seq -gt $uiLast){
                    $uiLast=[int]$command.seq
                    if($command.operation -eq 'arm'){$reply=[string]$excel.Run("'"+$uiOwner.Name+"'!OracleProbe.ArmUI",[string]$command.id,[double]$command.value,[int]$command.delay)}
                    elseif($command.operation -eq 'collect'){$reply=[string]$excel.Run("'"+$uiOwner.Name+"'!OracleProbe.CollectUI",[string]$command.id);$runValue += "`n"+$reply}
                    else {throw 'Unrecognized UI probe command'}
                    @{seq=$uiLast;command=$command;reply=$reply;tick=[Environment]::TickCount64;utc=[DateTime]::UtcNow.ToString('o')}|ConvertTo-Json -Depth 5 -Compress|Add-Content (Join-Path $uiRoot ('ui-journal-'+$uiRun+'.jsonl'))
                }
            }
            Start-Sleep -Milliseconds 50
        }

'@
$uiHook=$uiHook.Replace('__PROBE__',$probe.Replace("'","''")).Replace('__RUN__',$RunId.Replace("'","''"))
$hookAt=$workerSource.IndexOf('        $guardianHealthy = $guardian -and -not $guardian.HasExited')
if($hookAt -lt 0){throw 'UI observation insertion point absent'}
$workerSource=$workerSource.Insert($hookAt,$uiHook)
}
$guardianSource = Get-Content -Raw -LiteralPath (Join-Path $OxVbaRoot 'scripts/excel-vba-oracle-guardian.ps1')
$guardianSource = $guardianSource.Replace('$PSScriptRoot', "'" + (Join-Path $OxVbaRoot 'scripts') + "'")
$guardianAnchor = '            $observedProcessId = [int]$window.Current.ProcessId'
$guardianSource = $guardianSource.Replace($guardianAnchor, $guardianAnchor + @'

            # A workbook window may close between enumeration and capture. It
            # cannot authorize an observation or dismissal after its PID is gone.
            if ($observedProcessId -eq 0) {
                [ordered]@{hwnd=$windowHandle; utc=[DateTime]::UtcNow.ToString('o'); action='skip-vanished-window-no-interaction'} |
                    ConvertTo-Json -Compress | Add-Content -LiteralPath ($EventsFile + '.vanished.jsonl') -Encoding utf8NoBOM
                continue
            }
'@)
$guardianPath = Join-Path $probe 'selected-guardian.ps1'
[IO.File]::WriteAllText($guardianPath, $guardianSource)
$workerSource = $workerSource.Replace('(Join-Path ' + "'" + (Join-Path $OxVbaRoot 'scripts') + "' " + '"excel-vba-oracle-guardian.ps1")', "'" + $guardianPath.Replace("'", "''") + "'")
$workerPath = Join-Path $probe 'selected-worker.ps1'
[IO.File]::WriteAllText($workerPath, $workerSource)
$supervisor = $supervisor.Replace("(Join-Path '" + (Join-Path $OxVbaRoot 'scripts') + "' " + '"excel-vba-oracle-worker.ps1")', "'" + $workerPath.Replace("'", "''") + "'")
# The original per-case cap is 120 seconds. This probe batches fresh workbook
# clones; retain the existing supervisor timeout and increase only its case cap.
$supervisor = $supervisor.Replace('[Math]::Min(120, $TimeoutSeconds)', '[Math]::Min(1500, $TimeoutSeconds)')
$scriptPath = Join-Path $probe 'selected-supervisor.ps1'
[IO.File]::WriteAllText($scriptPath, $supervisor)
$arguments = @('-NoProfile','-File',$scriptPath,'-EnvironmentId','win-x64-dev-oracle-2026-07','-NoMatrixUpdate','-DiagnosticCaseId','windows-scalar-interop','-RunId',$RunId,'-OutputRoot',(Join-Path $probe 'raw'),'-TimeoutSeconds','1500')
if ($SafeMode) { $arguments += '-SafeMode' }
$helper=$null
try {
    if($ExternalController){
        $helperArguments=@('-NoProfile','-STA','-File',(Join-Path $probe 'external-controller.ps1'),'-ProbeDirectory',$probe,'-RunId',$RunId,'-OxVbaRoot',$OxVbaRoot)
        $helper=Start-Process -FilePath (Join-Path $PSHOME 'pwsh.exe') -ArgumentList $helperArguments -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $probe ("external-"+$RunId+'.stdout.txt')) -RedirectStandardError (Join-Path $probe ("external-"+$RunId+'.stderr.txt'))
    }
    & (Join-Path $PSHOME 'pwsh.exe') @arguments
    $supervisorExit = $LASTEXITCODE
}finally{
    $evidenceSource=Join-Path $probe ('raw/'+$RunId+'/driver-source')
    New-Item -ItemType Directory -Path $evidenceSource -Force|Out-Null
    foreach($sourcePath in @($PSCommandPath,$common,$selectedSource,$module,$workerPath,$guardianPath,$scriptPath,(Join-Path $probe 'machine.json'),$native,(Join-Path $probe 'external-controller.ps1'))){
        if(Test-Path -LiteralPath $sourcePath){Copy-Item -LiteralPath $sourcePath -Destination $evidenceSource -Force}
    }
    if($helper){
        [IO.File]::WriteAllText((Join-Path $probe ("external-"+$RunId+'.stop')),'stop')
        if(-not $helper.WaitForExit(5000)){$helper.Kill();$helper.WaitForExit()}
        [ordered]@{pid=$helper.Id;start_utc=$helper.StartTime.ToUniversalTime().ToString('o');exited=$helper.HasExited}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $probe ("external-"+$RunId+'.cleanup.json')) -Encoding utf8NoBOM
    }
}
$results = @(Get-ChildItem -LiteralPath (Join-Path $probe 'raw') -Filter '*.json' -Recurse | Where-Object Name -eq 'results.json')
Write-Output "Supervisor exit $supervisorExit; result files $($results.Count); measurement descriptor intentionally has no expected result."
