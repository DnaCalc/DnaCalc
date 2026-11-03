param([string]$RunDirectory = (Join-Path $PSScriptRoot ('runs/' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))), [switch]$NoTrace, [string]$X1Xll, [string]$HostExecutable,
    [string]$ExcelDnaDirectory, [string]$AddInName='X0', [string]$Cases, [switch]$Reload, [switch]$ReloadNewHost,
    [switch]$AotAliasLoad, [switch]$ExpectAotReloadRefusal,
    [string]$AddInDirectory, [string]$XllFileName, [switch]$ExpectXloilReloadRefusal, [switch]$Engine,
    [switch]$PrivateComAddIn)
$ErrorActionPreference = 'Stop'
if ([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA') { throw 'Run with pwsh -Sta -File run.ps1.' }
if ($ReloadNewHost) { $Reload=$true }
if ($Engine -and -not $HostExecutable) { throw '-Engine needs -HostExecutable.' }
if ($PrivateComAddIn) {
    if (-not $AddInDirectory) { throw '-PrivateComAddIn needs an explicitly selected external fixture.' }
    $principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
    if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'The unchanged Excel-DNA COM helper must run without administrator membership so it uses private HKCU classes.'
    }
}
if ($ExpectAotReloadRefusal -and ((-not $Reload -and -not $AotAliasLoad) -or -not $HostExecutable)) { throw '-ExpectAotReloadRefusal requires a reload/alias attempt and -HostExecutable.' }
if ($ExpectXloilReloadRefusal -and ($ExpectAotReloadRefusal -or (-not $Reload -and -not $AotAliasLoad) -or -not $HostExecutable)) { throw '-ExpectXloilReloadRefusal requires a host reload/alias attempt without the AOT expectation.' }
$expectedRefusal=if($ExpectAotReloadRefusal){'dc-snz.18: reinitializing a packed Native AOT runtime already attempted'}elseif($ExpectXloilReloadRefusal){'dc-snz.21: reopening the measured xlOil 0.22.1 runtime after teardown is not modelled'}else{$null}
$run = [IO.Path]::GetFullPath($RunDirectory)
if (Test-Path -LiteralPath $run) { throw 'Use a new run directory; earlier evidence is never overwritten.' }
New-Item -ItemType Directory -Path $run -Force | Out-Null
$sourceFiles = [ordered]@{}
$managedFiles = [ordered]@{}
if ($AddInDirectory) {
    if ($X1Xll -or $ExcelDnaDirectory -or -not $Cases -or -not $XllFileName) { throw 'An external add-in needs -XllFileName and -Cases, without another fixture.' }
    if ([IO.Path]::GetFileName($XllFileName) -ne $XllFileName -or [IO.Path]::GetExtension($XllFileName) -ne '.xll') { throw 'XllFileName must be a simple .xll file name.' }
    Get-ChildItem -LiteralPath (Resolve-Path -LiteralPath $AddInDirectory).Path -File | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $run
        $managedFiles[$_.Name] = (Get-FileHash -LiteralPath $_.FullName).Hash
    }
    $managedXll = Join-Path $run $XllFileName
    if (-not (Test-Path -LiteralPath $managedXll)) { throw 'Missing external XLL.' }
    $Cases = (Resolve-Path -LiteralPath $Cases).Path
}
if ($ExcelDnaDirectory) {
    if ($X1Xll) { throw 'Select either the X1 fixture or an Excel-DNA fixture.' }
    if ($AddInName -notmatch '^[A-Za-z0-9]+$') { throw 'AddInName must be a simple fixture name.' }
    Get-ChildItem -LiteralPath (Resolve-Path -LiteralPath $ExcelDnaDirectory).Path -File | Where-Object { $_.Name.StartsWith($AddInName) -and $_.Extension -in '.xll','.dna','.dll','.json' } | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $run
        $managedFiles[$_.Name] = (Get-FileHash -LiteralPath $_.FullName).Hash
    }
    $managedXll = Join-Path $run "$AddInName-AddIn64.xll"
    if (-not (Test-Path -LiteralPath $managedXll)) { throw 'Missing original Excel-DNA XLL.' }
    if (-not $Cases) {
        $Cases = Join-Path $PSScriptRoot ('fixture/' + $(if($AddInName -eq 'X0'){'x2-cases.json'}elseif($AddInName -eq 'X2Pool'){'x2-pool-cases.json'}else{'x2-hello-cases.json'}))
    }
    $Cases = (Resolve-Path -LiteralPath $Cases).Path
}
if ($X1Xll -or $ExcelDnaDirectory -or $AddInDirectory) {
    $repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
    foreach ($directory in 'crates/calc-xll-abi','crates/calc-xll-host','crates/calc-xll-fixture','crates/calc-com-host','platform/calc-xll-shim','apps/dna-calc-desktop','crates/calc-fml','crates/calc-syntax','crates/calc-func','crates/calc-value','apps/dna-onecalc','crates/calc-engine','crates/calc-host','crates/calc-doc-model','crates/calc-doc') {
        Get-ChildItem -LiteralPath (Join-Path $repositoryRoot $directory) -Recurse -File | Where-Object Extension -in '.rs','.toml','.c','.cpp','.rc','.h','.hpp' | ForEach-Object {
            $sourceFiles[[IO.Path]::GetRelativePath($repositoryRoot, $_.FullName).Replace('\','/')] = (Get-FileHash -LiteralPath $_.FullName).Hash
        }
    }
    foreach ($file in 'Cargo.toml','Cargo.lock','crates/calc-xll-reference/native/owned.cpp','crates/calc-xll-reference/build.ps1') { $sourceFiles[$file]=(Get-FileHash -LiteralPath (Join-Path $repositoryRoot $file)).Hash }
    Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'fixture') -File | ForEach-Object { $sourceFiles[[IO.Path]::GetRelativePath($repositoryRoot, $_.FullName).Replace('\','/')] = (Get-FileHash -LiteralPath $_.FullName).Hash }
}
if ($HostExecutable) {
    if (-not $X1Xll -and -not $ExcelDnaDirectory -and -not $AddInDirectory) { throw '-HostExecutable requires an XLL fixture.' }
    $builtExe = (Resolve-Path -LiteralPath $HostExecutable).Path
    # A capture owns its executable copy; a later build cannot replace its code,
    # and the capture cannot hold the build output open while Cargo links it.
    $exe = Join-Path $run ([IO.Path]::GetFileName($builtExe))
    Copy-Item -LiteralPath $builtExe -Destination $exe
    Copy-Item -LiteralPath (Join-Path (Split-Path $builtExe) 'xlcall32.dll') -Destination (Join-Path $run 'xlcall32.dll')
    if ($ExcelDnaDirectory -or $AddInDirectory) { $nativeXll = $managedXll } else {
        $nativeXll = Join-Path $run 'X1.xll'
        Copy-Item -LiteralPath (Resolve-Path -LiteralPath $X1Xll).Path -Destination $nativeXll
    }
    $trace = Join-Path $run 'boundary.jsonl'; $stop = Join-Path $run 'detach.request'
    $readyFile = Join-Path $run 'host.ready'; $go = Join-Path $run 'host.go'
    $done = Join-Path $run 'host.finished'; $release = Join-Path $run 'host.release'
    $hostProcess = $null; $debugger = $null; $detached = $false
    try {
        $launch = [Diagnostics.ProcessStartInfo]::new($exe)
        $launch.UseShellExecute=$false; $launch.CreateNoWindow=$true
        $launch.RedirectStandardOutput=$true; $launch.RedirectStandardError=$true
        $arguments = @('--xll',$nativeXll,'--ready',$readyFile,'--wait',$go,'--finished',$done,'--release',$release)
        if ($Engine) { $arguments += '--engine' }
        if ($ExcelDnaDirectory -or $AddInDirectory) {
            $arguments += @('--cases',$Cases,'--sheet','X0')
            if ($XllFileName -ieq 'xlOil.xll' -and (Get-Content -LiteralPath $Cases -Raw) -match 'X[45]\.(Py|Rtd)') {
                $arguments += @('--startup-ack',(Join-Path $run 'x4-startup-ready'))
            }
            if (-not $AddInDirectory -and $AddInName -eq 'X0') {
                $ownerXll = Join-Path $run 'NativeOwner64.xll'
                Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'build/NativeOwner64.xll') -Destination $ownerXll
                $arguments += @('--xll',$ownerXll,'--command','X0.Queue','--command','X0.Context','--early-command','X0.Queue')
            }
            if ($Reload) { $arguments += '--reload' }
            if ($ReloadNewHost) { $arguments += '--reload-new-host' }
            if ($AotAliasLoad) {
                $alias=Join-Path $run 'Alias-AddIn64.xll'
                Copy-Item -LiteralPath $nativeXll -Destination $alias
                $arguments += @('--xll',$alias)
            }
        } else { $arguments += '--self-test' }
        foreach ($argument in $arguments) { [void]$launch.ArgumentList.Add($argument) }
        $hostProcess = [Diagnostics.Process]::Start($launch)
        $stdout = $hostProcess.StandardOutput.ReadToEndAsync(); $stderr = $hostProcess.StandardError.ReadToEndAsync()
        $hostStartTicks = $hostProcess.StartTime.ToUniversalTime().Ticks
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        while (-not (Test-Path -LiteralPath $readyFile)) {
            if ($hostProcess.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Native host did not become ready; inspect host.stderr.' }
            Start-Sleep -Milliseconds 100
        }
        $metadata = [ordered]@{
            captured_utc=[DateTime]::UtcNow.ToString('O'); pid=$hostProcess.Id; process_bits=64; workbook_name='Book1'
            native_xll_sha256=(Get-FileHash -LiteralPath $nativeXll).Hash
            native_source_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot '../calc-xll-fixture/src/fixture.rs')).Hash
            cases_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'fixture/x1-cases.json')).Hash
            executable_sha256=(Get-FileHash -LiteralPath $exe).Hash
            executable_version=(Get-Item -LiteralPath $exe).VersionInfo.FileVersion
            shim_sha256=(Get-FileHash -LiteralPath (Join-Path (Split-Path $exe) 'xlcall32.dll')).Hash
            tracer_sha256=if($NoTrace){$null}else{(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'build/trace.exe')).Hash}
            tracer_source_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'native/trace.cpp')).Hash
            repository_commit=(& git -C $PSScriptRoot rev-parse HEAD)
            repository_dirty=[bool](& git -C $PSScriptRoot status --porcelain)
            mode=if($NoTrace){'DNA Calc control without debugger'}else{'DNA Calc debugger boundary trace'}
            source_files=$sourceFiles
            host_arguments=$arguments
        }
        if ($ExcelDnaDirectory -or $AddInDirectory) {
            $metadata.fixture_files=$managedFiles; $metadata.fixture_name=if($AddInDirectory){$XllFileName}else{$AddInName}
            $metadata.fixture_framework=if($AddInDirectory){'external unmodified add-in: ' + $AddInDirectory}else{$ExcelDnaDirectory}
            $metadata.cases_sha256=(Get-FileHash -LiteralPath $Cases).Hash
            $metadata.reload=[bool]$Reload
            $metadata.reload_new_host=[bool]$ReloadNewHost
            $metadata.aot_alias_load=[bool]$AotAliasLoad
            $metadata.assume_excel_process='1 (set by the host before creating threads)'
            if (-not $AddInDirectory -and $AddInName -eq 'X0') { $metadata.owner_xll_sha256=(Get-FileHash -LiteralPath $ownerXll).Hash }
        }
        $metadata.runner_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash
        $metadata | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $run 'environment.json') -Encoding utf8NoBOM
        if (-not $NoTrace) {
            $debugger = Start-Process -FilePath (Join-Path $PSScriptRoot 'build/trace.exe') -WindowStyle Hidden -PassThru -ArgumentList @(
                "$($hostProcess.Id)", "`"$trace`"", "`"$stop`"", "`"$exe`"") `
                -RedirectStandardOutput (Join-Path $run 'tracer.stdout') -RedirectStandardError (Join-Path $run 'tracer.stderr')
            $deadline = [DateTime]::UtcNow.AddSeconds(30)
            do {
                if ($debugger.HasExited) { throw 'Tracer exited before attach; inspect tracer.stderr.' }
                $attached = (Test-Path -LiteralPath $trace) -and ((Get-Content -LiteralPath $trace -Raw) -match '"event":"ready"')
                if (-not $attached) { Start-Sleep -Milliseconds 100 }
            } until ($attached -or [DateTime]::UtcNow -gt $deadline)
            if (-not $attached) { throw 'Native host tracer attach timed out.' }
        }
        Set-Content -LiteralPath $go -Value 'go'
        $deadline = [DateTime]::UtcNow.AddSeconds(600)
        while (-not (Test-Path -LiteralPath $done)) {
            if ($expectedRefusal -and $hostProcess.HasExited) { break }
            if ($hostProcess.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Native host did not finish; inspect host.stderr.' }
            Start-Sleep -Milliseconds 100
        }
    } finally {
        if ($debugger -and -not $debugger.HasExited) {
            Set-Content -LiteralPath $stop -Value 'detach'
            $detached = $debugger.WaitForExit(15000)
        } else { $detached = $true }
        if ($detached -and $hostProcess) {
            Set-Content -LiteralPath $release -Value 'release'
            if (-not $hostProcess.WaitForExit(10000)) {
                $remaining = Get-Process -Id $hostProcess.Id -ErrorAction SilentlyContinue
                if ($remaining -and $remaining.StartTime.ToUniversalTime().Ticks -eq $hostStartTicks) { Stop-Process -Id $hostProcess.Id }
                throw 'Native host did not exit after release.'
            }
            $stdout.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $run 'host.stdout') -Encoding utf8NoBOM
            $stderr.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $run 'host.stderr') -Encoding utf8NoBOM
            [ordered]@{exit_code=$hostProcess.ExitCode; finished=(Test-Path -LiteralPath $done)} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $run 'host.exit.json') -Encoding utf8NoBOM
        }
    }
    if (-not $detached) { throw 'Tracer did not detach; owned host left running for inspection.' }
    if ($expectedRefusal) {
        $message=Get-Content -LiteralPath (Join-Path $run 'host.stderr') -Raw
        if ($hostProcess.ExitCode -ne 1 -or -not $message.Contains($expectedRefusal) -or $message -match 'Unhandled exception') { throw 'Expected a typed runtime reload refusal; inspect host.stderr.' }
        [ordered]@{status='typed refusal, not a compatibility pass';exit_code=$hostProcess.ExitCode;message=$message.Trim()} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $run 'refusal.json') -Encoding utf8NoBOM
    } elseif ($hostProcess.ExitCode -ne 0) { throw 'Native host returned failure; inspect host.stderr.' }
    if ($debugger -and $debugger.ExitCode -ne 0) { throw 'Native host tracer reported an incomplete capture.' }
    Write-Output $run
    return
}
if ($ExcelDnaDirectory -or $AddInDirectory) { $xll=$managedXll } else {
    $fixture = Join-Path $PSScriptRoot 'fixture/bin/Release/net48'
    foreach ($file in 'X0-AddIn64.xll','X0-AddIn64.dna','X0.dll') { Copy-Item -LiteralPath (Join-Path $fixture $file) -Destination $run }
    $xll = Join-Path $run 'X0-AddIn64.xll'
}
$nativeXll = Join-Path $run 'NativeOwner64.xll'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'build/NativeOwner64.xll') -Destination $nativeXll
if ($X1Xll) {
    $nativeXll = Join-Path $run 'X1.xll'
    Copy-Item -LiteralPath (Resolve-Path -LiteralPath $X1Xll).Path -Destination $nativeXll
}
$trace = Join-Path $run 'boundary.jsonl'
$stop = Join-Path $run 'detach.request'
$baseline = @(Get-Process EXCEL -ErrorAction SilentlyContinue | ForEach-Object Id)
$excel = $null; $ownedExcelPid = 0; $ownedStartTicks = 0; $debugger = $null; $book = $null; $bookClosed=$false
$detachTimedOut = $false
$observations = [Collections.Generic.List[object]]::new()
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class X0Windows {
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
}
'@
function Note([string]$Operation, $Value) {
    $item = [ordered]@{utc=[DateTime]::UtcNow.ToString('O'); operation=$Operation; value=$Value}
    $observations.Add($item)
    $item | ConvertTo-Json -Compress -Depth 8 | Add-Content -LiteralPath (Join-Path $run 'controller.jsonl') -Encoding utf8NoBOM
}
try {
    $excel = New-Object -ComObject Excel.Application
    [uint32]$observedExcelPid = 0
    [void][X0Windows]::GetWindowThreadProcessId([IntPtr]$excel.Hwnd, [ref]$observedExcelPid)
    if (-not $observedExcelPid -or $observedExcelPid -eq 3144 -or $observedExcelPid -in $baseline) {
        throw "Excel activation did not produce an isolated PID ($observedExcelPid); no Quit or Stop-Process will be used."
    }
    $ownedExcelPid = [int]$observedExcelPid
    $ownedStartTicks = (Get-Process -Id $ownedExcelPid).StartTime.ToUniversalTime().Ticks
    Set-Content -LiteralPath (Join-Path $run 'owned-excel.pid') -Value $ownedExcelPid
    $excel.DisplayAlerts = $false
    $book = $excel.Workbooks.Add()
    $sheet = $book.Worksheets.Item(1)
    $sheet.Name = 'X0'
    if ($X1Xll) { $sheet.Name = 'X1' }
    $metadata = [ordered]@{
        captured_utc=[DateTime]::UtcNow.ToString('O'); pid=$ownedExcelPid; preexisting_excel_pids=$baseline
        excel_version=[string]$excel.Version; excel_build=[string]$excel.Build
        workbook_name=[string]$book.Name
        excel_file_version=(Get-Item -LiteralPath (Get-Process -Id $ownedExcelPid).Path).VersionInfo.FileVersion
        process_bits=64; fixture_framework='ExcelDna.AddIn 1.9.0 NuGet; net48'
        xll_sha256=(Get-FileHash -LiteralPath $xll).Hash
        fixture_dll_sha256=if(Test-Path -LiteralPath (Join-Path $run 'X0.dll')){(Get-FileHash -LiteralPath (Join-Path $run 'X0.dll')).Hash}else{$null}
        integration='embedded in NuGet ExcelDna64.xll'
        tracer_sha256=if($NoTrace){$null}else{(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'build/trace.exe')).Hash}
        tracer_source_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'native/trace.cpp')).Hash
        fixture_source_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'fixture/Probe.cs')).Hash
        fixture_project_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'fixture/X0.csproj')).Hash
        native_source_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'native/owned.cpp')).Hash
        native_xll_sha256=(Get-FileHash -LiteralPath $nativeXll).Hash
        repository_commit=(& git -C $PSScriptRoot rev-parse HEAD); culture=(Get-Culture).Name
        repository_dirty=[bool](& git -C $PSScriptRoot status --porcelain)
        multi_threaded_enabled=[bool]$excel.MultiThreadedCalculation.Enabled
        multi_thread_count=[int]$excel.MultiThreadedCalculation.ThreadCount
        mode=if($NoTrace){'control without debugger'}else{'debugger boundary trace'}
    }
    if ($X1Xll) {
        $metadata.fixture_framework = 'original Rust X1 fixture; no managed add-in loaded'
        $metadata.native_source_sha256 = (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot '../calc-xll-fixture/src/fixture.rs')).Hash
        $metadata.cases_sha256 = (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'fixture/x1-cases.json')).Hash
        $metadata.automation_security_before = [int]$excel.AutomationSecurity
        $metadata.source_files = $sourceFiles
    }
    if ($ExcelDnaDirectory -or $AddInDirectory) {
        $metadata.fixture_files=$managedFiles; $metadata.fixture_name=if($AddInDirectory){$XllFileName}else{$AddInName}
        $metadata.cases_sha256=(Get-FileHash -LiteralPath $Cases).Hash
        $metadata.fixture_framework=if($AddInDirectory){'external unmodified add-in: ' + $AddInDirectory}else{$ExcelDnaDirectory}
        $metadata.source_files=$sourceFiles
        $metadata.automation_security_before=[int]$excel.AutomationSecurity
        $metadata.owner_xll_sha256=(Get-FileHash -LiteralPath $nativeXll).Hash
    }
    $metadata.runner_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash
    $metadata | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $run 'environment.json') -Encoding utf8NoBOM
    if (-not $NoTrace) {
        $debugger = Start-Process -FilePath (Join-Path $PSScriptRoot 'build/trace.exe') -ArgumentList @(
            "$ownedExcelPid", "`"$trace`"", "`"$stop`"") -WindowStyle Hidden -PassThru `
            -RedirectStandardOutput (Join-Path $run 'tracer.stdout') -RedirectStandardError (Join-Path $run 'tracer.stderr')
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            if ($debugger.HasExited) { throw "Tracer exited before attach: $($debugger.ExitCode)" }
            $ready = (Test-Path -LiteralPath $trace) -and ((Get-Content -LiteralPath $trace -Raw) -match '"event":"ready"')
            if (-not $ready) { Start-Sleep -Milliseconds 100 }
        } until ($ready -or [DateTime]::UtcNow -gt $deadline)
        if (-not $ready) { throw 'Tracer attach did not finish.' }
    }
    if ($ExcelDnaDirectory -or $AddInDirectory) {
        if ($PrivateComAddIn -and [int]$excel.AutomationSecurity -ne 1) {
            throw 'The unchanged ribbon helper would change AutomationSecurity; this capture is refused before loading.'
        }
        if (-not $excel.RegisterXLL($xll)) { throw 'Excel refused the selected add-in.' }
        Note 'RegisterXLL managed' $true
        if (-not $AddInDirectory -and $AddInName -eq 'X0') { Note 'RegisterXLL native' ([bool]$excel.RegisterXLL($nativeXll)) }
        if ($XllFileName -ieq 'xlOil.xll' -and (Get-Content -LiteralPath $Cases -Raw) -match 'X[45]\.(Py|Rtd)') {
            # The original plugin queues its built-in module import. That import
            # temporarily suppresses Python events, independently of EnableEvents.
            # The fixture acknowledges a delivered AfterCalculate after that
            # import is observed. The log alone precedes event resumption.
            $deadline=[DateTime]::UtcNow.AddSeconds(240)
            $pluginLog=Join-Path $run 'xlOil.log'
            do {
                [void]$excel.Ready
                $importObserved=(Test-Path -LiteralPath $pluginLog) -and
                    ((Get-Content -LiteralPath $pluginLog -Raw) -match "Loaded python module 'xloil\.excelfuncs'")
                if ($importObserved) { $excel.Calculate() }
                $startupComplete=$importObserved -and (Test-Path -LiteralPath (Join-Path $run 'x4-startup-ready'))
                if (-not $startupComplete) { Start-Sleep -Milliseconds 100 }
            } until ($startupComplete -or [DateTime]::UtcNow -gt $deadline)
            if (-not $startupComplete) { throw 'xlOil built-in Python startup did not complete.' }
            Note 'xlOil built-in Python startup complete' $true
        }
        $sheet.Range('A1').Value2 = 7
        foreach ($case in (Get-Content -LiteralPath $Cases -Raw | ConvertFrom-Json)) {
            if ($case.PSObject.Properties['manual'] -and $case.manual) { $excel.Calculation=-4135 }
            if ($case.PSObject.Properties['dependent']) {
                if ($case.PSObject.Properties['legacy'] -and $case.legacy) { $sheet.Range('C3').Formula=[string]$case.dependent }
                else { $sheet.Range('C3').Formula2=[string]$case.dependent }
            }
            if ($case.PSObject.Properties['enable_events']) { $excel.EnableEvents=[bool]$case.enable_events }
            if ($case.PSObject.Properties['edit']) { $sheet.Range([string]$case.edit.cell).Value2=[double]$case.edit.value }
            # Assign the formula anchor directly, as the OneCalc controller does.
            # Excel removes the previous anchor's spill when replacing its formula.
            if ($case.PSObject.Properties['legacy'] -and $case.legacy) { $sheet.Range('B3').Formula = [string]$case.formula }
            else { $sheet.Range('B3').Formula2 = [string]$case.formula }
            if ($case.PSObject.Properties['before_calculate']) {
                $commandValue=$excel.Run([string]$case.before_calculate)
                Note ('before Calculate ' + [string]$case.before_calculate) $commandValue
                if ($commandValue -ne $case.before_calculate_value) { throw 'Precalculation command differs.' }
            }
            if ($case.PSObject.Properties['manual'] -and $case.manual) { $excel.Calculate() }
            if ($case.formula -match 'X0.Async') {
                $deadline=[DateTime]::UtcNow.AddSeconds(60)
                do { Start-Sleep -Milliseconds 100; $value=$sheet.Range('B3').Value2 } until ($value -eq 42 -or [DateTime]::UtcNow -gt $deadline)
            }
            if ($case.PSObject.Properties['rtd_wait_value']) {
                $deadline=[DateTime]::UtcNow.AddSeconds($(if($NoTrace){60}else{180}))
                do {
                    # A breakpoint-heavy Calculate loop must leave the original
                    # provider's background Python task a scheduling interval.
                    Start-Sleep -Milliseconds $(if($NoTrace){50}else{500})
                    try { [void]$excel.Ready; $excel.Calculate(); $value=$sheet.Range('B3').Value2 }
                    catch { if ($_.Exception.HResult -notin @(-2147418111,-2147417846)) { throw }; Note 'RTD COM retry' $_.Exception.HResult }
                } until ($value -eq $case.rtd_wait_value -or [DateTime]::UtcNow -gt $deadline)
                if ($value -ne $case.rtd_wait_value) { throw 'RTD value deadline expired.' }
            }
            if ($case.PSObject.Properties['rtd_idle_ms']) {
                # An explicit idle observation exercises the real heartbeat timer;
                # debugger overhead must not supply the wait implicitly.
                $idle = [Diagnostics.Stopwatch]::StartNew()
                Note ('RTD idle begin ' + [string]$case.formula) ([long]$case.rtd_idle_ms)
                while ($idle.ElapsedMilliseconds -lt [long]$case.rtd_idle_ms) {
                    Start-Sleep -Milliseconds 50
                    [void]$excel.Ready
                }
                Note ('RTD idle complete ' + [string]$case.formula) $idle.ElapsedMilliseconds
            }
            $value=$sheet.Range('B3').Value2
            if ($case.value -is [PSCustomObject] -and $case.value.PSObject.Properties['error']) {
                Note ('raw ' + [string]$case.formula) $value
                $value=@{error=[string]$sheet.Range('B3').Text}
            }
            Note ([string]$case.formula) $value
            if ($case.PSObject.Properties['dependent']) {
                $dependent=$sheet.Range('C3').Value2
                Note ('dependent ' + [string]$case.formula) $dependent
                if ($dependent -ne $case.dependent_value) { throw 'Dependent value differs.' }
            }
        }
        if (-not $AddInDirectory -and $AddInName -eq 'X0') {
            Note 'Application.Run X0.Queue' $excel.Run('X0.Queue')
            Note 'Application.Run X0.Context' $excel.Run('X0.Context')
            for($i=0;$i -lt 10;$i++){Start-Sleep -Milliseconds 100; [void]$excel.Ready}
            if ((Get-Content -LiteralPath (Join-Path $run 'fixture.tsv') -Raw) -notmatch "Outside 89`t") { throw 'Outside-context probes did not finish.' }
        }
        if ($AddInDirectory -and (Get-Content -LiteralPath $Cases -Raw) -match 'X5\.Rtd(?:Value)?\(') {
            # Retire the workbook's RTD client while its server module is still
            # loaded, as the host composition does before XLL teardown.
            $book.Close($false); $bookClosed=$true
            Note 'Close RTD workbook before detach' $true
            $deadline=[DateTime]::UtcNow.AddSeconds(5)
            do {
                [void]$excel.Ready
                $terminated=if($XllFileName -ieq 'xlOil.xll') {
                    (Test-Path -LiteralPath (Join-Path $run 'xlOil.log')) -and ((Get-Content -LiteralPath (Join-Path $run 'xlOil.log') -Raw) -match 'Server terminate received')
                } else {
                    (Test-Path -LiteralPath (Join-Path $run 'rtd.tsv')) -and ((Get-Content -LiteralPath (Join-Path $run 'rtd.tsv') -Raw) -match '(?m)^ServerTerminate\t')
                }
                if (-not $terminated) { Start-Sleep -Milliseconds 50 }
            } until ($terminated -or [DateTime]::UtcNow -gt $deadline)
            Note 'RTD termination before detach' $terminated
        }
        Note 'AutomationSecurity unchanged before teardown' ([int]$excel.AutomationSecurity -eq $metadata.automation_security_before)
        Note 'ExecuteExcel4Macro UNREGISTER(managed)' $excel.ExecuteExcel4Macro('UNREGISTER("' + $xll.Replace('"','""') + '")')
        if ($Reload) {
            # Repeat the same close/open sequence as the native host. Keep the
            # first and second worksheet observations separate in the trace.
            Note 'RegisterXLL reload' ([bool]$excel.RegisterXLL($xll))
            foreach ($case in (Get-Content -LiteralPath $Cases -Raw | ConvertFrom-Json)) {
                $sheet.Range('B3:Z10').ClearContents()
                $sheet.Range('B3').Formula2 = [string]$case.formula
                Note ('reload ' + [string]$case.formula) $sheet.Range('B3').Value2
            }
            Note 'ExecuteExcel4Macro UNREGISTER(reload)' $excel.ExecuteExcel4Macro('UNREGISTER("' + $xll.Replace('"','""') + '")')
        }
        if (-not $AddInDirectory -and $AddInName -eq 'X0') { Note 'ExecuteExcel4Macro UNREGISTER(native)' $excel.ExecuteExcel4Macro('UNREGISTER("' + $nativeXll.Replace('"','""') + '")') }
        Note 'AutomationSecurity unchanged' ([int]$excel.AutomationSecurity -eq $metadata.automation_security_before)
    } elseif ($X1Xll) {
        if (-not $excel.RegisterXLL($nativeXll)) { throw 'Excel refused the X1 fixture.' }
        Note 'RegisterXLL X1' $true
        $sheet.Range('A1').Value2 = 7
        $cases = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'fixture/x1-cases.json') -Raw | ConvertFrom-Json
        foreach ($case in $cases) {
            $sheet.Range('B3:Z10').ClearContents()
            $sheet.Range('B3').Formula2 = [string]$case.formula
            if ($case.formula -match 'MyAsync') {
                $deadline = [DateTime]::UtcNow.AddSeconds(20)
                do { Start-Sleep -Milliseconds 50; $value = $sheet.Range('B3').Value2 } until ($value -eq 42 -or [DateTime]::UtcNow -gt $deadline)
            }
            $value = $sheet.Range('B3').Value2
            if ($case.value -is [PSCustomObject] -and $case.value.PSObject.Properties['error']) {
                # Preserve the raw COM error as well as its worksheet spelling.
                Note ('raw ' + $case.formula) $value
                $value = @{error=[string]$sheet.Range('B3').Text}
            }
            Note ([string]$case.formula) $value
        }
        Note 'ExecuteExcel4Macro UNREGISTER(X1)' $excel.ExecuteExcel4Macro('UNREGISTER("' + $nativeXll.Replace('"','""') + '")')
        Note 'AutomationSecurity unchanged' ([int]$excel.AutomationSecurity -eq $metadata.automation_security_before)
    } else {
    Note 'RegisterXLL' ([bool]$excel.RegisterXLL($xll))
    Note 'RegisterXLL native' ([bool]$excel.RegisterXLL($nativeXll))
    $sheet.Range('A1').Value2 = 7
    $sheet.Range('B1').Formula = '=X0.Hello("world")'
    $sheet.Range('B2').Formula = '=X0.Reference(A1)'
    $sheet.Range('B3').Formula = '=X0.Caller()'
    $sheet.Range('B4').Formula = '=X0.Async(41)'
    $sheet.Range('D1:D32').FormulaR1C1 = '=X0.ThreadSafe(ROW())'
    Note 'Application.CalculateFull' $excel.CalculateFull()
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do { Start-Sleep -Milliseconds 200; $asyncValue=$sheet.Range('B4').Value2 } until ($asyncValue -eq 42 -or [DateTime]::UtcNow -gt $deadline)
    foreach ($address in 'B1','B2','B3','B4','D1','D32') { Note ("Value2 " + $address) $sheet.Range($address).Value2 }
    $sheet.Range('B5').Formula = '=SUM(X0.Big())'
    Note 'Value2 B5' $sheet.Range('B5').Value2
    $sheet.Range('B6').Formula = '=X0.NativeOwned()'
    Note 'Value2 B6' $sheet.Range('B6').Value2
    Note 'Application.Run X0.Queue' $excel.Run('X0.Queue')
    Note 'Application.Run X0.Context' $excel.Run('X0.Context')
    # Repeated object-model calls let the Excel main thread process QueueAsMacro messages.
    for($i=0;$i -lt 15;$i++){ Start-Sleep -Milliseconds 100; [void]$excel.Ready }
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $outsideDone = (Get-Content -LiteralPath (Join-Path $run 'fixture.tsv') -Raw) -match "Outside 89`t"
        if (-not $outsideDone) { Start-Sleep -Milliseconds 100; [void]$excel.Ready }
    } until ($outsideDone -or [DateTime]::UtcNow -gt $deadline)
    if (-not $outsideDone) { throw 'Outside-context probes did not finish.' }
    $saved = Join-Path $run 'probe.xlsx'
    $book.SaveAs($saved, 51)
    Note 'SaveAs' 'probe.xlsx'
    # Unload only this XLL. AddIns.Add enumerates unrelated installed add-ins.
    Note 'ExecuteExcel4Macro UNREGISTER(path)' $excel.ExecuteExcel4Macro('UNREGISTER("' + $xll.Replace('"','""') + '")')
    Note 'ExecuteExcel4Macro UNREGISTER(native)' $excel.ExecuteExcel4Macro('UNREGISTER("' + $nativeXll.Replace('"','""') + '")')
    }
} catch {
    Note 'failure' $_.Exception.ToString()
    throw
} finally {
    if ($debugger -and -not $debugger.HasExited) {
        Set-Content -LiteralPath $stop -Value 'detach'
        if (-not $debugger.WaitForExit(15000)) { $detachTimedOut=$true; Note 'cleanup failure' 'tracer did not detach; Excel remains running for inspection' }
        else { Note 'tracer exit' $debugger.ExitCode }
    }
    if ($ownedExcelPid -and (-not $debugger -or $debugger.HasExited)) {
        if ($book -and -not $bookClosed) { try { $book.Close($false) } catch { Note 'Close exception' $_.Exception.Message } }
        if ($excel) { try { $excel.Quit() } catch { Note 'Quit exception' $_.Exception.Message } }
        if ($sheet) { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($sheet) }
        if ($book) { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($book) }
        if ($excel) { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($excel) }
        [GC]::Collect(); [GC]::WaitForPendingFinalizers()
        Start-Sleep -Milliseconds 500
        $remaining = Get-Process -Id $ownedExcelPid -ErrorAction SilentlyContinue
        if ($remaining -and $remaining.StartTime.ToUniversalTime().Ticks -eq $ownedStartTicks) {
            Stop-Process -Id $ownedExcelPid
            Note 'cleanup' ("Stopped owned Excel PID " + $ownedExcelPid)
        } else { Note 'cleanup' 'Owned Excel exited' }
    }
}
if ($detachTimedOut) { throw 'Tracer detach timed out; owned Excel remains running for inspection.' }
if ($debugger -and $debugger.HasExited -and $debugger.ExitCode -ne 0) { throw "Tracer reported incomplete capture (exit $($debugger.ExitCode))." }
Write-Output $run
