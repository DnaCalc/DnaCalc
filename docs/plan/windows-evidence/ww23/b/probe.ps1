param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$src=Get-Content "$PSScriptRoot/b.vba" -Raw
$cases=@(5,1,2,3,4,6,7,8,10,11,12,13,14,20,21,22,23,24,25,26,27,28,30,31)
foreach($k in $cases) {
  $b=Book
  try {
    $m=$b.VBProject.VBComponents.Add(1); $m.Name='LaneProbe'; $m.CodeModule.AddFromString($src)
    if($k -eq 13 -or $k -eq 14){ $null=$excel.Run("'$($b.Name)'!LaneProbe.Probe",10) }
    Observe "VBA.case$k" { $ret=$excel.Run("'$($b.Name)'!LaneProbe.Probe",[int]$k); @{vba=$ret;active=$excel.ActiveWorkbook.Name} }
  } finally { foreach($x in @($excel.Workbooks)){ try{$x.Close($false)}catch{} } }
}
