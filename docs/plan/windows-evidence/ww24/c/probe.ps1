param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$src=Get-Content "$PSScriptRoot/c.vba" -Raw
foreach($k in @(2)) {
  $b=Book
  try {
    $m=$b.VBProject.VBComponents.Add(1); $m.Name='LaneProbe'; $m.CodeModule.AddFromString($src)
    Observe "VBA.case$k" { $ret=$excel.Run("'$($b.Name)'!LaneProbe.Probe",[int]$k); @{vba=$ret} }
  } finally { foreach($x in @($excel.Workbooks)){ try{$x.Close($false)}catch{} } }
}
if(Test-Path "$env:TEMP\ww24c4.xlsx"){ Copy-Item "$env:TEMP\ww24c4.xlsx" "$Directory/c4.xlsx" -Force }


