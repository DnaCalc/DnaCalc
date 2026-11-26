param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$src=Get-Content "$PSScriptRoot/g.vba" -Raw
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$hw=[int]$excel.Hwnd
$rs=[runspacefactory]::CreateRunspace(); $rs.Open()
$ps=[powershell]::Create(); $ps.Runspace=$rs
$null=$ps.AddScript({ param($hw,$out)
  Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
  $el=[System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$hw); $pid_=$el.Current.ProcessId
  $cond=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty,$pid_)
  $end=(Get-Date).AddMinutes(8)
  while((Get-Date) -lt $end){
    try{
      foreach($w in [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children,$cond)){
        if($w.Current.ClassName -eq '#32770'){
          $texts=@($w.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|%{ $_.Current.ControlType.ProgrammaticName+':'+$_.Current.Name })
          (@{title=$w.Current.Name;items=$texts;time=(Get-Date).ToString('o')}|ConvertTo-Json -Compress) | Add-Content "$out/dialogs.jsonl"
          $btn=$w.FindFirst([System.Windows.Automation.TreeScope]::Descendants,(New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,'Cancel')))
          if(-not $btn){ $btn=$w.FindFirst([System.Windows.Automation.TreeScope]::Descendants,(New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,'OK'))) }
          if($btn){ ($btn.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke() }
        }
      }
    }catch{}
    Start-Sleep -Milliseconds 400
  }
}).AddArgument($hw).AddArgument($Directory)
$h=$ps.BeginInvoke()
Remove-Item "$env:TEMP\ww24g*.xlsx" -ea 0
try {
foreach($k in @(10)) {
  $b=Book
  try {
    $m=$b.VBProject.VBComponents.Add(1); $m.Name='LaneProbe'; $m.CodeModule.AddFromString($src)
    Observe "VBA.case$k" { $ret=$excel.Run("'$($b.Name)'!LaneProbe.Probe",[int]$k); @{vba=$ret} }
  } finally { foreach($x in @($excel.Workbooks)){ try{$x.Saved=$true;$x.Close($false)}catch{} } }
}
} finally { $ps.Stop(); $rs.Close() }
Get-ChildItem "$env:TEMP\ww24g*.xlsx" | % { Copy-Item $_.FullName "$Directory/$($_.Name)" -Force }


