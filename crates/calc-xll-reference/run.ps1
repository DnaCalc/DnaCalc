param([string]$RunDirectory = (Join-Path $PSScriptRoot ('runs/' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))), [switch]$NoTrace, [string]$X1Xll, [string]$HostExecutable)
$ErrorActionPreference = 'Stop'
if ([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA') { throw 'Run with pwsh -Sta -File run.ps1.' }
$run = [IO.Path]::GetFullPath($RunDirectory)
if (Test-Path -LiteralPath $run) { throw 'Use a new run directory; earlier evidence is never overwritten.' }
New-Item -ItemType Directory -Path $run -Force | Out-Null
$sourceFiles = [ordered]@{}
if ($X1Xll) {
    $repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
    foreach ($directory in 'crates/calc-xll-abi','crates/calc-xll-host','crates/calc-xll-fixture','platform/calc-xll-shim','apps/dna-calc-desktop') {
        Get-ChildItem -LiteralPath (Join-Path $repositoryRoot $directory) -Recurse -File | Where-Object Extension -in '.rs','.toml','.c','.rc' | ForEach-Object {
            $sourceFiles[[IO.Path]::GetRelativePath($repositoryRoot, $_.FullName).Replace('\','/')] = (Get-FileHash -LiteralPath $_.FullName).Hash
        }
    }
}
if ($HostExecutable) {
    if (-not $X1Xll) { throw '-HostExecutable requires -X1Xll.' }
    $exe = (Resolve-Path -LiteralPath $HostExecutable).Path
    $nativeXll = Join-Path $run 'X1.xll'
    Copy-Item -LiteralPath (Resolve-Path -LiteralPath $X1Xll).Path -Destination $nativeXll
    $trace = Join-Path $run 'boundary.jsonl'; $stop = Join-Path $run 'detach.request'
    $readyFile = Join-Path $run 'host.ready'; $go = Join-Path $run 'host.go'
    $done = Join-Path $run 'host.finished'; $release = Join-Path $run 'host.release'
    $hostProcess = $null; $debugger = $null; $detached = $false
    try {
        $hostProcess = Start-Process -FilePath $exe -WindowStyle Hidden -PassThru -ArgumentList @(
            '--xll', "`"$nativeXll`"", '--self-test', '--ready', "`"$readyFile`"", '--wait', "`"$go`"", '--finished', "`"$done`"", '--release', "`"$release`"") `
            -RedirectStandardOutput (Join-Path $run 'host.stdout') -RedirectStandardError (Join-Path $run 'host.stderr')
        $hostStartTicks = $hostProcess.StartTime.ToUniversalTime().Ticks
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        while (-not (Test-Path -LiteralPath $readyFile)) {
            if ($hostProcess.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Native host did not become ready; inspect host.stderr.' }
            Start-Sleep -Milliseconds 100
        }
        $metadata = [ordered]@{
            captured_utc=[DateTime]::UtcNow.ToString('O'); pid=$hostProcess.Id; process_bits=64
            native_xll_sha256=(Get-FileHash -LiteralPath $nativeXll).Hash
            native_source_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot '../calc-xll-fixture/src/fixture.rs')).Hash
            cases_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'fixture/x1-cases.json')).Hash
            executable_sha256=(Get-FileHash -LiteralPath $exe).Hash
            executable_version=(Get-Item -LiteralPath $exe).VersionInfo.FileVersion
            shim_sha256=(Get-FileHash -LiteralPath (Join-Path (Split-Path $exe) 'xlcall32.dll')).Hash
            tracer_sha256=if($NoTrace){$null}else{(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'build/trace.exe')).Hash}
            tracer_source_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'native/trace.cpp')).Hash
            repository_commit=(& git -C $PSScriptRoot rev-parse HEAD)
            mode=if($NoTrace){'DNA Calc control without debugger'}else{'DNA Calc debugger boundary trace'}
            source_files=$sourceFiles
        }
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
        $deadline = [DateTime]::UtcNow.AddSeconds(120)
        while (-not (Test-Path -LiteralPath $done)) {
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
        }
    }
    if (-not $detached) { throw 'Tracer did not detach; owned host left running for inspection.' }
    if ($hostProcess.ExitCode -ne 0) { throw 'Native host returned failure; inspect host.stderr.' }
    if ($debugger -and $debugger.ExitCode -ne 0) { throw 'Native host tracer reported an incomplete capture.' }
    Write-Output $run
    return
}
$fixture = Join-Path $PSScriptRoot 'fixture/bin/Release/net48'
foreach ($file in 'X0-AddIn64.xll','X0-AddIn64.dna','X0.dll') {
    Copy-Item -LiteralPath (Join-Path $fixture $file) -Destination $run
}
$xll = Join-Path $run 'X0-AddIn64.xll'
$nativeXll = Join-Path $run 'NativeOwner64.xll'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'build/NativeOwner64.xll') -Destination $nativeXll
if ($X1Xll) {
    $nativeXll = Join-Path $run 'X1.xll'
    Copy-Item -LiteralPath (Resolve-Path -LiteralPath $X1Xll).Path -Destination $nativeXll
}
$trace = Join-Path $run 'boundary.jsonl'
$stop = Join-Path $run 'detach.request'
$baseline = @(Get-Process EXCEL -ErrorAction SilentlyContinue | ForEach-Object Id)
$excel = $null; $ownedExcelPid = 0; $ownedStartTicks = 0; $debugger = $null; $book = $null
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
        excel_file_version=(Get-Item -LiteralPath (Get-Process -Id $ownedExcelPid).Path).VersionInfo.FileVersion
        process_bits=64; fixture_framework='ExcelDna.AddIn 1.9.0 NuGet; net48'
        xll_sha256=(Get-FileHash -LiteralPath $xll).Hash
        fixture_dll_sha256=(Get-FileHash -LiteralPath (Join-Path $run 'X0.dll')).Hash
        integration='embedded in NuGet ExcelDna64.xll'
        tracer_sha256=if($NoTrace){$null}else{(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'build/trace.exe')).Hash}
        tracer_source_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'native/trace.cpp')).Hash
        fixture_source_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'fixture/Probe.cs')).Hash
        fixture_project_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'fixture/X0.csproj')).Hash
        native_source_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'native/owned.cpp')).Hash
        native_xll_sha256=(Get-FileHash -LiteralPath $nativeXll).Hash
        repository_commit=(& git -C $PSScriptRoot rev-parse HEAD); culture=(Get-Culture).Name
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
    if ($X1Xll) {
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
        if ($book) { try { $book.Close($false) } catch { Note 'Close exception' $_.Exception.Message } }
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
