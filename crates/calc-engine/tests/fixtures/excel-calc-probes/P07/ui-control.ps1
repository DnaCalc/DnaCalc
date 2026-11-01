param([Parameter(Mandatory)][int]$Seq,[Parameter(Mandatory)][ValidateSet('arm','collect')][string]$Operation,[Parameter(Mandatory)][string]$Id,[double]$Value=42,[int]$Delay=2000,[string]$RunId='P07-ui2-safe',[string]$ProbeDirectory=$PSScriptRoot)
$ErrorActionPreference='Stop'
$path=Join-Path $ProbeDirectory ('ui-command-'+$RunId+'.json')
@{seq=$Seq;operation=$Operation;id=$Id;value=$Value;delay=$Delay}|ConvertTo-Json|Set-Content ($path+'.tmp')
[IO.File]::Move($path+'.tmp',$path,$true)
$deadline=[DateTime]::UtcNow.AddSeconds(40)
do {
    $journal=Join-Path $ProbeDirectory ('ui-journal-'+$RunId+'.jsonl')
    if(Test-Path $journal){$last=Get-Content $journal -Tail 1|ConvertFrom-Json;if($last.seq -eq $Seq){Write-Output ('ack '+$Seq+' '+$Operation+' '+$Id);exit}}
    Start-Sleep -Milliseconds 100
} while([DateTime]::UtcNow -lt $deadline)
throw 'UI measurement command did not acknowledge'
