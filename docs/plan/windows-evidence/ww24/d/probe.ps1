param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false; $ev=$excel.EnableEvents
$src=Get-Content "$PSScriptRoot/d.vba" -Raw
foreach($k in @(2)) {
  $b=Book
  try {
    $m=$b.VBProject.VBComponents.Add(1); $m.Name='LaneProbe'; $m.CodeModule.AddFromString($src)
    Observe "VBA.case$k" { $ret=$excel.Run("'$($b.Name)'!LaneProbe.Probe",[int]$k); @{vba=$ret} }
  } finally { $excel.Calculation=-4105; $excel.EnableEvents=$ev; $excel.CalculateBeforeSave=$true; foreach($x in @($excel.Workbooks)){ try{$x.Saved=$true;$x.Close($false)}catch{} } }
}


