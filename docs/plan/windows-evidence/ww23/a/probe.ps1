param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
function Names($b){ @(Tabs $b) }
# 1 Worksheets.Add no args naming
$b=Book
try {
 Observe 'Add.noargs' { $s=$b.Worksheets.Add(); @{name=$s.Name;idx=$s.Index;active=$excel.ActiveSheet.Name;tabs=@(Names $b)} }
 Observe 'Add.delete-then-add' { $b.Worksheets.Item('Sheet1').Delete(); $s=$b.Worksheets.Add(); @{name=$s.Name;tabs=@(Names $b)} }
 Observe 'Add.type-chart' { $s=$b.Worksheets.Add($missing,$missing,1,-4109); @{name=$s.Name;tn=$s.GetType().Name;tabs=@(Names $b)} }
 Observe 'Add.count2' { $s=$b.Worksheets.Add($missing,$missing,2); @{name=$s.Name;tabs=@(Names $b);active=$excel.ActiveSheet.Name} }
 Observe 'Add.count3' { $s=$b.Worksheets.Add($missing,$missing,3); @{name=$s.Name;tabs=@(Names $b);active=$excel.ActiveSheet.Name;sel=$excel.ActiveWindow.SelectedSheets.Count} }
} finally { $b.Close($false) }
# delete only visible
$b=Book
try {
 Observe 'Delete.onlySheet' { $b.Worksheets.Item(1).Delete(); @{n=$b.Worksheets.Count} }
 Observe 'Delete.onlyVisible' { $s=$b.Worksheets.Add(); $h=$b.Worksheets.Add(); $h.Visible=0; $s.Visible=-1; $x=$b.Worksheets.Count; $b.Worksheets.Item(1).Visible=0; $b.Worksheets.Item(2).Visible=0 ; 'prep' }
} finally { $b.Close($false) }
$b=Book
try {
 $s=$b.Worksheets.Item(1); $t=$b.Worksheets.Add($missing,$s); $t.Visible=0
 Observe 'Delete.lastVisible' { $s.Delete(); @{n=$b.Worksheets.Count;tabs=@(Names $b)} }
} finally { $b.Close($false) }
# Range.Insert / Delete
$b=Book
try {
 $s=$b.Worksheets.Item(1)
 $s.Range('A1:C3').Value2=1
 Observe 'Insert.ret' { $r=$s.Range('B2:C3').Insert(); @{ret=$r;a=$s.Range('B2').Value2;d=$s.Range('B4').Value2} }
 $s.Cells.Clear() | Out-Null
 $s.Range('A1:C6').Value2=1
 Observe 'Insert.wide.shiftdefault' { $s.Range('A1:C1').Insert() | Out-Null; @{a3=$s.Range('A1').Value2;a2=$s.Range('A2').Value2;a7=$s.Range('A7').Value2} }
 $s.Cells.Clear() | Out-Null
 $s.Range('A1:E5').Value2=1
 Observe 'Insert.tall.shiftdefault' { $s.Range('A1:A3').Insert() | Out-Null; @{a1=$s.Range('A1').Value2;b1=$s.Range('B1').Value2;f1=$s.Range('F1').Value2;a6=$s.Range('A6').Value2} }
 $s.Cells.Clear() | Out-Null
 $s.Range('A1:E5').Value2=1
 Observe 'Delete.ret' { $r=$s.Range('B2:C3').Delete(); @{ret=$r;b2=$s.Range('B2').Value2;b4=$s.Range('B4').Value2;d2=$s.Range('D2').Value2} }
 $s.Cells.Clear() | Out-Null
 $s.Range('A1:E5').Value2=1
 Observe 'Delete.wide.default' { $s.Range('A1:C1').Delete() | Out-Null; @{a1=$s.Range('A1').Value2;a5=$s.Range('A5').Value2;a4=$s.Range('A4').Value2} }
 $s.Cells.Clear() | Out-Null
 $s.Range('A1:E5').Value2=1
 Observe 'Delete.tall.default' { $s.Range('A1:A3').Delete() | Out-Null; @{a1=$s.Range('A1').Value2;b1=$s.Range('B1').Value2;e1=$s.Range('E1').Value2;a4=$s.Range('A4').Value2} }
 # CopyOrigin
 $s.Cells.Clear() | Out-Null
 $s.Range('E40').Interior.ColorIndex=3; $s.Range('E42').Interior.ColorIndex=5
 Observe 'Insert.CopyOrigin1' { $s.Range('E41').Insert(-4121,1) | Out-Null; @{e41=$s.Range('E41').Interior.ColorIndex;e42=$s.Range('E42').Interior.ColorIndex;e43=$s.Range('E43').Interior.ColorIndex} }
 $s.Cells.Clear() | Out-Null
 $s.Range('E40').Interior.ColorIndex=3; $s.Range('E41').Interior.ColorIndex=5
 Observe 'Insert.CopyOrigin0' { $s.Range('E41').Insert(-4121,0) | Out-Null; @{e41=$s.Range('E41').Interior.ColorIndex;e42=$s.Range('E42').Interior.ColorIndex} }
 $s.Cells.Clear() | Out-Null
 $s.Range('E40').Interior.ColorIndex=3; $s.Range('E41').Interior.ColorIndex=5
 Observe 'Insert.Row.CopyOrigin1' { $s.Rows.Item(41).Insert(-4121,1) | Out-Null; @{e41=$s.Range('E41').Interior.ColorIndex;e42=$s.Range('E42').Interior.ColorIndex} }
} finally { $b.Close($false) }
# Find
$b=Book
try {
 $s=$b.Worksheets.Item(1)
 $s.Range('A1').Value2='apple';$s.Range('B1').Value2='pie';$s.Range('C1').Value2='x'
 $s.Range('A2').Value2='Apple pie';$s.Range('B2').Value2='fig';$s.Range('C2').Value2=12.5
 $s.Range('A3').Value2='banana';$s.Range('B3').Value2='APPLE';$s.Range('C3').Formula='=1+1'
 $rg=$s.Range('A1:C3')
 function Addr($r){ if($r -eq $null){'Nothing'}else{$r.Address()} }
 Observe 'Find.apple.default' { Addr ($rg.Find('apple')) }
 Observe 'Find.apple.xlpart.xlvalues' { Addr ($rg.Find('apple',$missing,-4163,2)) }
 Observe 'Find.afterOutside' { Addr ($rg.Find('apple',$s.Range('E5'))) }
 Observe 'Find.afterMulti' { Addr ($rg.Find('apple',$s.Range('A1:B2'))) }
 Observe 'Find.whatOmitted' { Addr ($rg.Find()) }
 $s.Range('D1').Formula='=""'
 Observe 'Find.star.emptytext' { Addr ($s.Range('D1').Find('*',$missing,-4163)) }
 Observe 'Find.star.emptytext.formulas' { Addr ($s.Range('D1').Find('*',$missing,-4123)) }
 $s.Range('E1').Value2=45000; $s.Range('E1').NumberFormat='yyyy-mm-dd'
 $s.Range('E2').Value2=12.5; $s.Range('E2').NumberFormat='$#,##0.00'
 Observe 'Find.date.text' { @{a=(Addr ($s.Range('E1:E2').Find('2023-03-15',$missing,-4163,2))); b=(Addr ($s.Range('E1:E2').Find('45000',$missing,-4163,2)));c=(Addr ($s.Range('E1:E2').Find('$12.50',$missing,-4163,2)));d=(Addr ($s.Range('E1:E2').Find('12.5',$missing,-4163,2)))} }
 Observe 'Find.saved.settings' {
   $r1=$rg.Find('apple',$missing,-4163,1,1,1,$false)   # xlWhole
   $a1=Addr $r1
   $r2=$rg.Find('apple')   # LookAt omitted
   $a2=Addr $r2
   $r3=$rg.Find('apple',$missing,$missing,$missing,$missing,$missing,$true)
   $a3=Addr $r3
   $r4=$rg.Find('apple')
   @{whole=$a1;omitted=$a2;matchcase=$a3;afterMatchCase=(Addr $r4)} }
 Observe 'FindNext.beforeFind' { $s2=$b.Worksheets.Add(); $s2.Range('A1').Value2='q'; Addr ($s2.Range('A1:B2').FindNext()) }
 Observe 'FindPrevious.afterFirst' { Addr ($rg.FindPrevious($s.Range('A1'))) }
 Observe 'Find.singlecell' { @{hit=(Addr ($s.Range('A1').Find('apple')));miss=(Addr ($s.Range('A1').Find('pie')))} }
 $s3=$b.Worksheets.Add()
 $s3.Range('A1').Formula2='=SEQUENCE(3)'
 Observe 'Find.spill' { @{a=(Addr ($s3.Cells.Find('2',$missing,-4163,1)));b=(Addr ($s3.Cells.Find('3',$missing,-4163,1)))} }
} finally { $b.Close($false) }
