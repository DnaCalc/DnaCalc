param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
function Seed($s){ $s.Range('A1').Value2='k';$s.Range('B1').Value2='v'; $s.Range('A2').Value2='a';$s.Range('B2').Value2=1;$s.Range('A3').Value2='b';$s.Range('B3').Value2=2;$s.Range('A4').Value2='c';$s.Range('B4').Value2=3 }
function Emb($s,[int]$type=51){ $co=$s.ChartObjects().Add(10,10,300,200); $ch=$co.Chart; $ch.SetSourceData($s.Range('A1:B4')); $ch.ChartType=$type; $ch }
# 1 Charts.Add
$b=Book
try {
 $s=$b.Worksheets.Item(1); Seed $s
 Observe 'Charts.Add.noSelection' { $c=$b.Charts.Add(); @{name=$c.Name;idx=$c.Index;active=$excel.ActiveSheet.Name;series=$c.SeriesCollection().Count;tabs=@(Tabs $b);count=$b.Charts.Count} }
 Observe 'Charts.Add.second' { $c=$b.Charts.Add(); @{name=$c.Name;idx=$c.Index;tabs=@(Tabs $b)} }
 Observe 'Charts.Add.count3' { $c=$b.Charts.Add($missing,$missing,3); @{name=$c.Name;tabs=@(Tabs $b);active=$excel.ActiveSheet.Name} }
 $s.Activate(); $s.Range('A1:B4').Select()
 Observe 'Charts.Add.withSelection' { $c=$b.Charts.Add(); @{name=$c.Name;series=$c.SeriesCollection().Count;type=$c.ChartType;idx=$c.Index} }
 foreach($n in 0,256,-1){ Observe "Sheets.Add.chart.count$n" { $x=$b.Sheets.Add($missing,$missing,$n,-4109); @{name=$x.Name} } }
 Observe 'Sheets.Add.chart.BeforeAfter' { $x=$b.Sheets.Add($b.Sheets.Item(1),$b.Sheets.Item(2),1,-4109); @{name=$x.Name} }
 Observe 'Charts.Add.BeforeAfter' { $x=$b.Charts.Add($b.Sheets.Item(1),$b.Sheets.Item(2)); @{name=$x.Name} }
 Observe 'Sheets.Add.dialog' { $d=$b.Sheets.Add($missing,$missing,1,-4116); @{name=$d.Name} }
 Observe 'Counts' { @{worksheets=$b.Worksheets.Count;charts=$b.Charts.Count;sheets=$b.Sheets.Count;dialogs=$b.DialogSheets.Count;tabs=@(Tabs $b)} }
} finally { $b.Close($false) }
$vba=@'
Public Function Probe(k As Long) As String
 Dim x As Object, s As String
 Dim ch As Chart, co As ChartObject
 On Error GoTo Failed
 Select Case k
 Case 1
  Set x = ActiveWorkbook.Charts.Add: x.Name = "Chart1"
  Set x = ActiveWorkbook.Sheets.Add(Type:=xlDialogSheet)
  Probe = "ws " & Worksheets.Count & " ch " & Charts.Count & " sh " & Sheets.Count
  Probe = Probe & " tn " & TypeName(Sheets("Chart1")) & "/" & TypeName(Sheets(Sheets.Count - 1)) & "/" & TypeName(Sheets(1))
 Case 2
  ActiveWorkbook.Charts.Add
  Set x = Worksheets("Chart1")
 Case 3
  ActiveWorkbook.Charts.Add
  Set x = Charts("Sheet1")
 Case 10
  ActiveSheet.Range("A1:B4").Value = 1
  Set co = ActiveSheet.ChartObjects.Add(10, 10, 300, 200)
  co.Chart.SetSourceData ActiveSheet.Range("A1:B4")
  Probe = "co.Name=" & co.Name & "|chart.Name=" & co.Chart.Name
 Case 11
  ActiveSheet.Range("A1:B4").Value = 1
  Set ch = ActiveWorkbook.Charts.Add
  Probe = "cs.Name=" & ch.Name
  Set ch = ch.Location(xlLocationAsObject, "Sheet1")
  Probe = Probe & "|after location: charts " & ActiveWorkbook.Charts.Count & " tabs " & ActiveWorkbook.Sheets.Count & " co=" & ActiveSheet.ChartObjects.Count
  Set co = ActiveSheet.ChartObjects(1)
  Probe = Probe & "|co " & co.Left & "," & co.Top & "," & co.Width & "," & co.Height & " name " & co.Name & " chart.Name " & ch.Name
 Case 12
  ActiveSheet.Range("A1:B4").Value = 1
  Set ch = ActiveWorkbook.Charts.Add
  Set ch = ch.Location(xlLocationAsObject, "NoSuchSheet")
 Case 13
  ActiveSheet.Range("A1:B4").Value = 1
  Set co = ActiveSheet.ChartObjects.Add(10, 10, 300, 200)
  co.Chart.SetSourceData ActiveSheet.Range("A1:B4")
  Set ch = co.Chart.Location(xlLocationAsNewSheet)
  Probe = "newsheet name " & ch.Name & " tabs " & ActiveWorkbook.Sheets.Count & " cos " & Worksheets(1).ChartObjects.Count
 Case 14
  ActiveSheet.Range("A1:B4").Value = 1
  Set co = ActiveSheet.ChartObjects.Add(10, 10, 300, 200)
  co.Chart.SetSourceData ActiveSheet.Range("A1:B4")
  co.Chart.Name = "x"
  Probe = "name " & co.Chart.Name
 Case 15
  ActiveSheet.Range("A1:B4").Value = 1
  Set ch = ActiveWorkbook.Charts.Add
  ch.Name = "x": Probe = "cs name " & ch.Name
 End Select
 Exit Function
Failed:
 Probe = Probe & " ERR " & Err.Number & "|" & Err.Description
End Function
'@
foreach($k in 1,2,3,10,11,12,13,14,15){
 $b=Book
 try {
  $m=$b.VBProject.VBComponents.Add(1); $m.Name='LaneProbe'; $m.CodeModule.AddFromString($vba)
  $b.Worksheets.Item(1).Range('A1:B4').Value2=1; $b.Worksheets.Item(1).Name='Sheet1'
  Observe "VBA.chart$k" { @{vba=$excel.Run("'$($b.Name)'!LaneProbe.Probe",[int]$k)} }
 } finally { foreach($x in @($excel.Workbooks)){ try{$x.Close($false)}catch{} } }
}
$b=Book
try {
 $s=$b.Worksheets.Item(1); Seed $s
 $ch=Emb $s 51
 Observe 'Axes.cat.MinimumScale' { $a=$ch.Axes(1); $a.MinimumScale }
 Observe 'Axes.cat.MaximumScale' { $a=$ch.Axes(1); $a.MaximumScale }
 Observe 'Axes.val.scale.auto' { $a=$ch.Axes(2); @{max=$a.MaximumScale;min=$a.MinimumScale;auto=$a.MaximumScaleIsAuto;minauto=$a.MinimumScaleIsAuto} }
 Observe 'Axes.val.HasTitle.default' { $a=$ch.Axes(2); @{has=$a.HasTitle} }
 Observe 'Axes.val.AxisTitle.noTitle' { $a=$ch.Axes(2); $a.AxisTitle.Text }
 Observe 'Axes.val.HasTitle.true' { $a=$ch.Axes(2); $a.HasTitle=$true; @{text=$a.AxisTitle.Text;cap=$a.AxisTitle.Caption} }
 Observe 'TickLabels.NumberFormat' { $a=$ch.Axes(2); @{nf=$a.TickLabels.NumberFormat;linked=$a.TickLabels.NumberFormatLinked;cat=$ch.Axes(1).TickLabels.NumberFormat} }
 Observe 'TickLabels.NumberFormat.putGeneral' { $a=$ch.Axes(2); $a.TickLabels.NumberFormat='General'; @{nf=$a.TickLabels.NumberFormat;linked=$a.TickLabels.NumberFormatLinked} }
 Observe 'TickLabels.NumberFormatLinked.false' { $a=$ch.Axes(1); $a.TickLabels.NumberFormatLinked=$false; @{nf=$a.TickLabels.NumberFormat;linked=$a.TickLabels.NumberFormatLinked} }
 Observe 'TickLabels.NumberFormat.empty' { $a=$ch.Axes(2); $a.TickLabels.NumberFormat=''; @{nf=$a.TickLabels.NumberFormat;linked=$a.TickLabels.NumberFormatLinked} }
 Observe 'TickLabels.NumberFormat.invalid' { $a=$ch.Axes(2); $a.TickLabels.NumberFormat='[[bad'; @{nf=$a.TickLabels.NumberFormat} }
 $pie=Emb $s 5
 Observe 'Pie.Axes1' { $pie.Axes(1).Type }
 Observe 'Pie.Axes2' { $pie.Axes(2).Type }
 Observe 'Pie.HasAxis' { @{h1=$pie.HasAxis(1)} }
 $col=Emb $s 51
 Observe 'Series.Interior.Color.auto' { $sr=$col.SeriesCollection(1); @{color=$sr.Interior.Color;ci=$sr.Interior.ColorIndex;rgb=$sr.Format.Fill.ForeColor.RGB} }
 foreach($v in -1,16777216,1.5){ Observe "Series.Interior.Color.put$v" { $sr=$col.SeriesCollection(1); $sr.Interior.Color=$v; @{color=$sr.Interior.Color} } }
 Observe 'ChartArea.Interior.Color' { @{ca=$col.ChartArea.Interior.Color;pa=$col.PlotArea.Interior.Color;caRGB=$col.ChartArea.Format.Fill.ForeColor.RGB;paRGB=$col.PlotArea.Format.Fill.ForeColor.RGB;caci=$col.ChartArea.Interior.ColorIndex;paci=$col.PlotArea.Interior.ColorIndex} }
 foreach($v in -1,16777216,1.5){ Observe "ChartArea.Interior.Color.put$v" { $col.ChartArea.Interior.Color=$v; $col.ChartArea.Interior.Color } ; Observe "PlotArea.Interior.Color.put$v" { $col.PlotArea.Interior.Color=$v; $col.PlotArea.Interior.Color } }
 $c2=Emb $s 51
 Observe 'ChartArea.Border' { $x=$c2.ChartArea.Border; @{ls=$x.LineStyle;w=$x.Weight;c=$x.Color;ci=$x.ColorIndex} }
 Observe 'PlotArea.Border' { $x=$c2.PlotArea.Border; @{ls=$x.LineStyle;w=$x.Weight;c=$x.Color;ci=$x.ColorIndex} }
 foreach($t in 51,57,4,1,76){ $cc=Emb $s $t; Observe "Series.Border.type$t" { $x=$cc.SeriesCollection(1).Border; @{ls=$x.LineStyle;w=$x.Weight;c=$x.Color;ci=$x.ColorIndex} } }
 foreach($ls in -4119,13,-4115,-4118,4,5,1,-4142){ Observe "ChartArea.Border.LineStyle$ls" { $c2.ChartArea.Border.LineStyle=$ls; @{ls=$c2.ChartArea.Border.LineStyle} } }
 foreach($w in 1,2,-4138,4){ Observe "ChartArea.Border.Weight$w" { $c2.ChartArea.Border.LineStyle=1; $c2.ChartArea.Border.Weight=$w; @{w=$c2.ChartArea.Border.Weight} } }
 # data labels
 foreach($t in 51,52,4,65,5,1,-4120){ $dc=Emb $s $t
   Observe "DataLabels.type$t" { $sr=$dc.SeriesCollection(1); $sr.HasDataLabels=$true; @{pos=$sr.DataLabels().Position;has=$sr.HasDataLabels} } }
 $dl=Emb $s 51
 Observe 'DataLabels.Index0' { $sr=$dl.SeriesCollection(1); $sr.HasDataLabels=$true; $sr.DataLabels(0).Text }
 Observe 'DataLabels.IndexPast' { $sr=$dl.SeriesCollection(1); $sr.DataLabels(9).Text }
 Observe 'DataLabel.Delete.ShowValue' { $sr=$dl.SeriesCollection(1); $p=$sr.DataLabels(2); $p.Delete(); $p.ShowValue }
 Observe 'ApplyDataLabels6' { $dl.SeriesCollection(1).ApplyDataLabels(6) }
 Observe 'HasDataLabels.false' { $sr=$dl.SeriesCollection(1); $sr.HasDataLabels=$false; @{has=$sr.HasDataLabels} }
 # export
 $ex=Emb $s 51
 $dir=$Directory
 foreach($f in 'PNG','GIF','JPG','BMP'){ Observe "Export.$f" { $p="$dir\x.$($f.ToLower())"; $r=$ex.Export($p,$f); $len=(Get-Item $p -ea 0).Length; $w=$null; if($f -eq 'PNG'){ $by=[IO.File]::ReadAllBytes($p); $w=@(([int]$by[16]*16777216+[int]$by[17]*65536+[int]$by[18]*256+$by[19]),([int]$by[20]*16777216+[int]$by[21]*65536+[int]$by[22]*256+$by[23])) }; @{ret=$r;len=$len;size=$w} } }
 Observe 'Export.noFilter' { $p="$dir\nf.png"; $r=$ex.Export($p); @{ret=$r;exists=(Test-Path $p)} }
 Observe 'Export.emptyPath' { $ex.Export('') }
 Observe 'Export.CO.size' { $co=$s.ChartObjects(1); @{w=$co.Width;h=$co.Height} }
} finally { $b.Close($false) }

