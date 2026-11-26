param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$src=Get-Content "$PSScriptRoot/f.vba" -Raw
Remove-Item "$env:TEMP\ww24f*.xlsx" -ea 0
foreach($k in 1..6) {
  $b=Book
  try {
    $m=$b.VBProject.VBComponents.Add(1); $m.Name='LaneProbe'; $m.CodeModule.AddFromString($src)
    Observe "VBA.case$k" { $ret=$excel.Run("'$($b.Name)'!LaneProbe.Probe",[int]$k); @{vba=$ret} }
  } finally { foreach($x in @($excel.Workbooks)){ try{$x.Saved=$true;$x.Close($false)}catch{} } }
}
Get-ChildItem "$env:TEMP\ww24f*.xlsx" | % { Copy-Item $_.FullName "$Directory/$($_.Name)" -Force }
