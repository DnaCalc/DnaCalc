param(
    [Parameter(Mandatory)][string]$ProbeDirectory,
    [string]$OxVbaRoot = 'C:/Work/DnaCalc/OxVba',
    [int]$Repetitions = 10,
    [string]$Cases = '',
    [string]$RunId = ('calc-probe-' + [Guid]::NewGuid().ToString('N')),
    [switch]$SafeMode
)
$ErrorActionPreference = 'Stop'
$probe = (Resolve-Path -LiteralPath $ProbeDirectory).Path
$common = Join-Path $PSScriptRoot 'calculation-common.bas'
$probeSource = Get-Content -LiteralPath (Join-Path $probe 'probe.bas') -Raw
$firstProcedure = [regex]::Match($probeSource, '(?m)^(?:Private|Public)\s+(?:Sub|Function)\b')
if (-not $firstProcedure.Success) {throw 'Probe has no procedures'}
$declarations = $probeSource.Substring(0,$firstProcedure.Index)
$procedures = $probeSource.Substring($firstProcedure.Index)
$source = (Get-Content -LiteralPath $common -Raw).Replace('Public Sub Note(', $declarations + "`nPublic Sub Note(") + "`n" + $procedures
$source = $source.Replace('__OUTPUT__', $probe.Replace('"','""')).Replace('__REPETITIONS__', [string]$Repetitions)
$source = $source.Replace('__CASES__', $Cases.Replace('"','""'))
$native = Join-Path (Split-Path $probe -Parent) 'P03/native-probe.xll'
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
& (Join-Path $PSHOME 'pwsh.exe') @arguments
$supervisorExit = $LASTEXITCODE
$results = @(Get-ChildItem -LiteralPath (Join-Path $probe 'raw') -Filter '*.json' -Recurse | Where-Object Name -eq 'results.json')
Write-Output "Supervisor exit $supervisorExit; result files $($results.Count); measurement descriptor intentionally has no expected result."
