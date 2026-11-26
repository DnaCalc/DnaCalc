param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$M=[Type]::Missing
function Grid($ws){ $ws.Range('A1').Value2='apple';$ws.Range('B1').Value2='pie';$ws.Range('C1').Value2='x';$ws.Range('A2').Value2='Apple pie';$ws.Range('B2').Value2='fig';$ws.Range('C2').Value2=12.5;$ws.Range('A3').Value2='banana';$ws.Range('B3').Value2='APPLE';$ws.Range('C3').Formula='=1+1' }
function Addr($r){ if($r -eq $null){'Nothing'}else{$r.Address($false,$false)} }
$b=Book
try{
 $w4=$b.Worksheets.Add(); $w4.Range('A1').Value2='name';$w4.Range('B1').Value2='qty'
 $n='a','b','c','d'; $q=5,15,25,35; for($i=0;$i -lt 4;$i++){$r=$i+2;$w4.Range("A$r").Value2=[string]$n[$i];$w4.Range("B$r").Value2=[double]$q[$i]}
 function Hid { ($w4.Rows.Item(2).Hidden,$w4.Rows.Item(3).Hidden,$w4.Rows.Item(4).Hidden,$w4.Rows.Item(5).Hidden) -join ',' }
 Observe 'af.noField_on' { $null=$w4.Range('A1').AutoFilter(); @{mode=$w4.AutoFilterMode;fm=$w4.FilterMode;addr=$w4.AutoFilter.Range.Address($false,$false)} }
 Observe 'af.noField_off' { $null=$w4.Range('A1').AutoFilter(); @{mode=$w4.AutoFilterMode} }
 Observe 'af.and' { $null=$w4.Range('A1:B5').AutoFilter(2,'>10',1,'<30'); @{hidden=(Hid);fm=$w4.FilterMode} }
 Observe 'af.showall' { $w4.ShowAllData(); @{hidden=(Hid);fm=$w4.FilterMode;mode=$w4.AutoFilterMode} }
 Observe 'af.showall_none' { $w4.ShowAllData() }
 Observe 'af.mode_true' { $w4.AutoFilterMode=$true }
 Observe 'af.top10' { $null=$w4.Range('A1:B5').AutoFilter(2,'2',3); @{hidden=(Hid)} }
 Observe 'af.values' { $null=$w4.Range('A1:B5').AutoFilter(2,[object[]]@('5','35'),7); @{hidden=(Hid)} }
 Observe 'af.field3' { $w4.Range('A1:B5').AutoFilter(3,'1') }
 Observe 'af.op20' { $w4.Range('A1:B5').AutoFilter(2,'1',20) }
 Observe 'af.top0' { $w4.Range('A1:B5').AutoFilter(2,'0',3) }
 Observe 'af.top501' { $w4.Range('A1:B5').AutoFilter(2,'501',3) }
 Observe 'af.top500' { $w4.Range('A1:B5').AutoFilter(2,'500',3); 'ok' }
 $b2=Book
 Observe 'add.default' { $s=$b2.Worksheets.Add(); @{name=$s.Name;idx=$s.Index;active=$b2.ActiveSheet.Name;tabs=(Tabs $b2) -join ','} }
 Observe 'add.after_delete_counter' { $b2.Worksheets.Item('Sheet1').Delete(); $s=$b2.Worksheets.Add(); @{name=$s.Name;tabs=(Tabs $b2) -join ','} }
 Observe 'add.count3' { $s=$b2.Worksheets.Add($b2.Worksheets.Item(1),$M,3); @{ret=$s.Name;active=$b2.ActiveSheet.Name;tabs=(Tabs $b2) -join ','} }
 foreach($c in 0,-1,256){ Observe "add.count$c" { $s=$b2.Worksheets.Add($M,$M,$c); $s.Name } }
 Observe 'add.chart' { $s=$b2.Worksheets.Add($M,$M,1,-4109); @{name=$s.Name;type=$s.Type;tabs=(Tabs $b2) -join ','} }
 Observe 'del.only_visible' { $b3=Book; try{ $b3.Worksheets.Item(1).Delete() } finally { $b3.Saved=$true; $b3.Close($false) } }
 Observe 'names.local' { $b4=Book; try{ $s1=$b4.Worksheets.Item(1); $s1.Name='My Sheet'; $b4.Names.Add('Zed','=1')|Out-Null; $x=$b4.Names.Add("'My Sheet'!Local",'=2'); $y=$b4.Names.Add('Sheet2!Loc2','=3'); @{n=$x.Name;n2=$y.Name;cnt=$b4.Names.Count;list=@($b4.Names|%{$_.Name});parent=$x.Parent.Name} } finally {$b4.Saved=$true;$b4.Close($false)} }
 Observe 'names.item_local' { $b4=Book; try{ $b4.Worksheets.Item(1).Names.Add('Local','=2')|Out-Null; $b4.Names.Item('Local').Name } finally {$b4.Saved=$true;$b4.Close($false)} }
 Observe 'names.unknown_sheet' { $b4=Book; try{ $b4.Names.Add('NoSuch!X','=1').Name } finally {$b4.Saved=$true;$b4.Close($false)} }
 $w5=$b2.Worksheets.Add(); $w5.Range('E40').Interior.Color=255; $w5.Range('E42').Interior.Color=65280; $w5.Range('E41').Value2=1
 Observe 'ins.ret' { $w5.Range('E41').Insert(-4121) }
 Observe 'ins.default_fmt' { $w5.Range('E41').Interior.Color }
 $w6=$b2.Worksheets.Add(); $w6.Range('E40').Interior.Color=255; $w6.Range('E42').Interior.Color=65280
 Observe 'ins.copyorigin1' { $null=$w6.Range('E41').Insert(-4121,1); @{e41=$w6.Range('E41').Interior.Color;e42=$w6.Range('E42').Interior.Color;e43=$w6.Range('E43').Interior.Color} }
 $w7=$b2.Worksheets.Add(); $w7.Range('A1:C3').Value2=1
 Observe 'ins.shift_omitted_wide' { $null=$w7.Range('A5:C6').Insert(); $w7.UsedRange.Address($false,$false) }
 Observe 'ins.shift_omitted_tall' { $w7.Range('E1:E3').Value2=1; $null=$w7.Range('G1:G3').Insert(); $w7.Range('G1:H3').Address($false,$false) }
 Observe 'del.ret' { $w7.Range('A1:B1').Delete() }
 $b5=Book
 Observe 'copy.names' { $s=$b5.Worksheets.Item(1); $s.Copy($M,$s); $s.Copy($M,$s); @{tabs=(Tabs $b5) -join ',';active=$b5.ActiveSheet.Name;idx=$b5.ActiveSheet.Index} }
 Observe 'copy.of_copy' { $b5.Worksheets.Item('Sheet1 (2)').Copy($M,$b5.Worksheets.Item(1)); @{tabs=(Tabs $b5) -join ','} }
 Observe 'copy.before' { $s=$b5.Worksheets.Item(1); $s.Copy($s); @{tabs=(Tabs $b5) -join ',';active=$b5.ActiveSheet.Name} }
 Observe 'copy.long31' { $s=$b5.Worksheets.Add(); $s.Name='ABCDEFGHIJKLMNOPQRSTUVWXYZ12345'; $s.Copy($M,$s); $b5.ActiveSheet.Name }
 Observe 'copy.long31_again' { $b5.Worksheets.Item('ABCDEFGHIJKLMNOPQRSTUVWXYZ12345').Copy($M,$b5.Worksheets.Item(1)); $b5.ActiveSheet.Name }
 Observe 'copy.newbook' { $n0=$excel.Workbooks.Count; $b5.Worksheets.Item(1).Copy(); $nb=$excel.ActiveWorkbook; @{newbook=($nb.Name -ne $b5.Name);tabs=(Tabs $nb) -join ',';count=$excel.Workbooks.Count-$n0} }
 Observe 'copy.chart_series' { $b6=Book; $s=$b6.Worksheets.Item(1); $s.Range('A1:A5').Value2=3; $co=$s.ChartObjects().Add(100,100,200,100); $co.Chart.SetSourceData($s.Range('A1:A5')); $s.Copy($M,$s); $c2=$b6.ActiveSheet.ChartObjects(1).Chart; $c2.SeriesCollection(1).Formula }
 Observe 'copy.array' { $b7=Book; $null=$b7.Worksheets.Add($M,$b7.Worksheets.Item(1)); $b7.Worksheets.Item([object[]]@('Sheet1','Sheet2')).Copy(); $nb=$excel.ActiveWorkbook; @{tabs=(Tabs $nb) -join ',';newbook=($nb.Name -ne $b7.Name)} }
 Observe 'copy.local_name_table_note' { $b8=Book; $s=$b8.Worksheets.Item(1); $s.Range('A1:B3').Value2=1; $s.Names.Add('Loc','=Sheet1!$A$1')|Out-Null; $s.Range('A1').AddComment('hi')|Out-Null; $s.Hyperlinks.Add($s.Range('D1'),'http://example.com')|Out-Null; $null=$s.ListObjects.Add(1,$s.Range('A1:B3'),$null,1); $s.Copy($M,$s); $c=$b8.ActiveSheet; @{name=$c.Name;names=@($c.Names|%{$_.Name});comments=$c.Comments.Count;links=$c.Hyperlinks.Count;tables=@($c.ListObjects|%{$_.Name})} }
} finally { foreach($x in @($excel.Workbooks)){ try{$x.Saved=$true;$x.Close($false)}catch{} } }
