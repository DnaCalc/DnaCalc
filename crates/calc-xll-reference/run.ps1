param([string]$RunDirectory = (Join-Path $PSScriptRoot ('runs/' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ'))), [switch]$NoTrace)
$ErrorActionPreference = 'Stop'
if ([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA') { throw 'Run with pwsh -Sta -File run.ps1.' }
$run = [IO.Path]::GetFullPath($RunDirectory)
if (Test-Path -LiteralPath $run) { throw 'Use a new run directory; earlier evidence is never overwritten.' }
New-Item -ItemType Directory -Path $run -Force | Out-Null
$fixture = Join-Path $PSScriptRoot 'fixture/bin/Release/net48'
foreach ($file in 'X0-AddIn64.xll','X0-AddIn64.dna','X0.dll') {
    Copy-Item -LiteralPath (Join-Path $fixture $file) -Destination $run
}
$xll = Join-Path $run 'X0-AddIn64.xll'
$nativeXll = Join-Path $run 'NativeOwner64.xll'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'build/NativeOwner64.xll') -Destination $nativeXll
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
