param([string]$SessionDirectory=$env:DNACALC_EXCEL_SESSION, [string]$Script, [string]$Parameters='{}',
    [string]$Environment='default', [string]$WorkingDirectory=(Get-Location).Path,
    [int]$TimeoutSeconds=900, [switch]$FreshRequired, [switch]$Stop, [switch]$Status,
    [ValidateRange(1,1000000)][int]$LeakEvery=50, [switch]$Start,
    [string]$PacketId=$env:DNACALC_EXCEL_PACKET, [switch]$NoSaveBaseline)
$ErrorActionPreference='Stop'
if (-not $SessionDirectory) { $SessionDirectory=Join-Path $PSScriptRoot '../../target/excel-session' }
$root=[IO.Path]::GetFullPath($SessionDirectory)
[void][IO.Directory]::CreateDirectory($root)
function Identity($Identity, $Name) {
    if (-not $Identity) { return $false }
    $p=Get-Process -Id $Identity.pid -ErrorAction SilentlyContinue
    return $p -and $p.ProcessName -ieq $Name -and $p.StartTime.ToUniversalTime().Ticks -eq $Identity.start_ticks
}
function Read-Status {
    if (-not (Test-Path -LiteralPath (Join-Path $root 'host.json'))) { return $null }
    $stream=[IO.File]::Open((Join-Path $root 'host.json'),[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try {
        $reader=[IO.StreamReader]::new($stream,[Text.Encoding]::UTF8,$true)
        try { $json=$reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally { $stream.Dispose() }
    ConvertFrom-Json -InputObject $json
}
function Live($State) { $State -and (Identity @{pid=$State.host_pid;start_ticks=$State.host_start_ticks} 'pwsh') }
if ($Status) { Read-Status|ConvertTo-Json -Depth 15; return }
if ($Stop) {
    [IO.File]::WriteAllText((Join-Path $root 'shutdown.request'),'stop')
    $deadline=[DateTime]::UtcNow.AddSeconds(30)
    while ((Live (Read-Status)) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    if (Live (Read-Status)) { throw 'Session host has not completed shutdown.' }
    return
}
if (-not $Script -and -not $Start) { throw '-Script is required.' }
$scriptPath=if($Script){(Resolve-Path -LiteralPath $Script).Path}else{$null}
$parametersObject=ConvertFrom-Json -InputObject $Parameters
$bytes=[Text.Encoding]::UTF8.GetBytes($root.ToLowerInvariant())
$mutexName='Local\DnaCalcExcelSession-'+[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes))
$mutex=[Threading.Mutex]::new($false,$mutexName)
if (-not $mutex.WaitOne(60000)) { throw 'Session startup lock timed out.' }
try {
    $state=Read-Status
    if (-not (Live $state)) {
        # Recover only a process with the exact ownership tuple from this host.
        if ($state -and $state.excel -and $state.excel.pid -ne 3144 -and (Identity $state.excel 'EXCEL')) {
            $command=(Get-CimInstance Win32_Process -Filter "ProcessId=$($state.excel.pid)").CommandLine
            if ($command -notmatch '(?i)/restore') { Stop-Process -Id $state.excel.pid }
        }
        Remove-Item -LiteralPath (Join-Path $root 'shutdown.request') -ErrorAction SilentlyContinue
        $hostScript=Join-Path $PSScriptRoot 'host.ps1'
        # ShellExecute creates a detached hidden console without inheriting the client's pipes.
        # RedirectStandardOutput on Start-Process made Rust's wait-for-EOF outlive the client.
        $hostArguments=@('-NoLogo','-NoProfile','-NonInteractive','-STA','-File',('"'+$hostScript+'"'),'-SessionDirectory',('"'+$root+'"'),'-LeakEvery',"$LeakEvery")
        if ($NoSaveBaseline) { $hostArguments+='-NoSaveBaseline' }
        $hostProcess=Start-Process -FilePath (Join-Path $PSHOME 'pwsh.exe') -WindowStyle Hidden -PassThru -ArgumentList $hostArguments
        $deadline=[DateTime]::UtcNow.AddSeconds(45)
        do {
            if ($hostProcess.HasExited) { throw ('Session host exited: '+(Get-Content (Join-Path $root 'host.stderr') -Raw)) }
            Start-Sleep -Milliseconds 100
            $state=Read-Status
        } until ((Live $state) -or [DateTime]::UtcNow -gt $deadline)
        if (-not (Live $state)) { throw 'Session host did not start.' }
    }
} finally { $mutex.ReleaseMutex(); $mutex.Dispose() }
if (-not $state.preserve_automation_security) {
    throw 'The running Excel host does not preserve AutomationSecurity; stop it before selecting this policy.'
}
if ([bool]$state.no_save_baseline -ne [bool]$NoSaveBaseline) {
    throw 'The running host has a different baseline saving policy; select a dedicated session directory.'
}
if($Start){Read-Status|ConvertTo-Json -Depth 15;return}
$id=[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffffff')+'-'+[Guid]::NewGuid().ToString('N')
$job=[ordered]@{id=$id;script=$scriptPath;parameters=$parametersObject;environment=$Environment;working_directory=[IO.Path]::GetFullPath($WorkingDirectory);fresh_required=[bool]$FreshRequired;timeout_seconds=$TimeoutSeconds;packet_id=$PacketId}
$jobFile=Join-Path $root ('queue/'+$id+'.json')
$temp=$jobFile+'.tmp'
[IO.File]::WriteAllText($temp,($job|ConvertTo-Json -Depth 30),[Text.UTF8Encoding]::new($false))
Move-Item -LiteralPath $temp -Destination $jobFile
$receipt=Join-Path $root ('completed/'+$id+'/receipt.json')
$deadline=[DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
while (-not (Test-Path -LiteralPath $receipt)) {
    if ([DateTime]::UtcNow -gt $deadline) {
        # Only the host's serialized deadline timer may stop an active job. Reading status and
        # then killing its process could race completion and terminate the next queued job.
        if (Test-Path -LiteralPath $jobFile) {
            try { Move-Item -LiteralPath $jobFile -Destination ($jobFile+'.cancelled') -ErrorAction Stop }
            catch { if(Test-Path -LiteralPath $jobFile){throw} }
        }
        throw "Excel session job timed out; evidence retained: $id"
    }
    $state=Read-Status
    if (-not (Live $state)) {
        if (Test-Path -LiteralPath $jobFile) { Move-Item -LiteralPath $jobFile -Destination ($jobFile+'.cancelled') }
        throw "Session host exited; evidence retained: $id"
    }
    Start-Sleep -Milliseconds 100
}
$result=Get-Content -LiteralPath $receipt -Raw -Encoding utf8|ConvertFrom-Json
Write-Output ('dnacalc-excel-session/1 receipt '+$receipt)
if (-not $result.success) { throw $result.error }
