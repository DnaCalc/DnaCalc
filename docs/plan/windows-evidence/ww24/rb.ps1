param([string]$Name,[int]$Seconds=540)
$root='C:\Work\DnaCalc\codex-runs\ww24'
Remove-Item "$root\$Name\observations.jsonl","$root\$Name\session" -Recurse -Force -ea 0
$p=Start-Process pwsh -ArgumentList '-NoProfile','-File',"$root\run.ps1",'-Name',$Name -PassThru -RedirectStandardOutput "$root\$Name\run.out" -NoNewWindow
if(-not $p.WaitForExit($Seconds*1000)){
  "TIMEOUT after $Seconds s" | Set-Content "$root\$Name\timeout.txt"
  try{ $h=Get-Content "$root\$Name\session\host.json" -Raw | ConvertFrom-Json
       $e=Get-Process -Id $h.excel.pid -ea 0; if($e -and $e.StartTime.ToUniversalTime().Ticks -eq $h.excel.start_ticks){ Stop-Process -Id $e.Id -Force }
       $hp=Get-Process -Id $h.host_pid -ea 0; if($hp -and $hp.StartTime.ToUniversalTime().Ticks -eq $h.host_start_ticks){ Stop-Process -Id $hp.Id -Force } }catch{}
  Stop-Process -Id $p.Id -Force -ea 0
}
if(Test-Path "$root\$Name\observations.jsonl"){ Get-Content "$root\$Name\observations.jsonl" } else { 'no observations' }

