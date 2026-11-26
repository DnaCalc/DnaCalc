param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$src=Get-Content "$PSScriptRoot/b.vba" -Raw
foreach($k in 1..5) {
  $b=Book
  try {
    $m=$b.VBProject.VBComponents.Add(1); $m.Name='LaneProbe'; $m.CodeModule.AddFromString($src)
    Observe "VBA.case$k" { $ret=$excel.Run("'$($b.Name)'!LaneProbe.Probe",[int]$k); @{vba=$ret} }
  } finally { foreach($x in @($excel.Workbooks)){ try{$x.Close($false)}catch{} } }
}
