param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
function Fresh { $b=Book; $s=$b.Worksheets.Item(1); $s.Name='Sheet1'; $s }
# ---- merges (dc-oc78o)
$b=Book
try {
 $s=$b.Worksheets.Item(1)
 $s.Range('B2').Value2=1;$s.Range('C2').Value2=2;$s.Range('B3').Value2=3;$s.Range('C3').Value2=4
 $s.Range('C2').Interior.ColorIndex=3; $s.Range('C3').Borders.Item(9).LineStyle=1
 Observe 'Merge.B2C3' { $s.Range('B2:C3').Merge(); @{b2=$s.Range('B2').Value2;c2=$s.Range('C2').Value2;b3=$s.Range('B3').Value2;c3=$s.Range('C3').Value2;mc=$s.Range('B2:D3').MergeCells;mcB2C3=$s.Range('B2:C3').MergeCells;c2fill=$s.Range('C2').Interior.ColorIndex;c3border=$s.Range('C3').Borders.Item(9).LineStyle} }
 Observe 'Merge.single' { $s.Range('A1').Merge(); @{mc=$s.Range('A1').MergeCells} }
 Observe 'MergeArea.addr' { @{a=$s.Range('B2:C3').MergeArea.Address();b=$s.Range('B2:D4').MergeArea.Address();c=$s.Range('C3').MergeArea.Address()} }
 Observe 'MergeArea.multiArea' { $s.Range('B2:C3,E5:F6').MergeArea.Address() }
 Observe 'MergeArea.multiArea.MergeCells' { $s.Range('B2:C3,E5:F6').MergeCells }
 Observe 'Merge.overlap' { $s.Range('C3:D4').Merge(); @{b2=$s.Range('B2').MergeCells;b2area=$s.Range('B2').MergeArea.Address();c3area=$s.Range('C3').MergeArea.Address();d4area=$s.Range('D4').MergeArea.Address()} }
 Observe 'MergeCells.text' { $s.Range('B2:C3').MergeCells='x'; 1 }
 Observe 'MergeCells.one' { $s.Range('G1:H2').MergeCells=1; @{mc=$s.Range('G1').MergeCells;area=$s.Range('G1').MergeArea.Address()} }
 Observe 'MergeCells.zero.unmerge' { $s.Range('G1:H2').MergeCells=$false; @{mc=$s.Range('G1').MergeCells} }
 $s.Range('J1').Formula2='=SEQUENCE(3)'
 Observe 'Merge.spill' { $s.Range('J1:K2').Merge(); 'ok' }
 $s.Range('M1').FormulaArray='=1+1'
 $s.Range('M1:M2').FormulaArray='={1;2}'
 Observe 'Merge.array' { $s.Range('M1:N2').Merge(); 'ok' }
 $lo=$s.ListObjects.Add(1,$s.Range('P1:Q3'),$missing,1)
 Observe 'Merge.table' { $s.Range('P2:Q3').Merge(); 'ok' }
 Observe 'Merge.tableOverlap' { $s.Range('O2:P2').Merge(); 'ok' }
} finally { $b.Close($false) }
# ---- borders (dc-oxh8w.33 cl5 2)
$b=Book
try {
 $s=$b.Worksheets.Item(1)
 Observe 'Border.neighbour' { $s.Range('B2:C3').Borders.Item(9).LineStyle=1; @{b4top=$s.Range('B4:C4').Borders.Item(8).LineStyle;b3bottom=$s.Range('B3').Borders.Item(9).LineStyle;c3bottom=$s.Range('C3').Borders.Item(9).LineStyle;b2bottom=$s.Range('B2').Borders.Item(9).LineStyle;rangeEdge=$s.Range('B2:C3').Borders.Item(9).LineStyle} }
 Observe 'Border.weightOnly' { $r=$s.Range('F2'); $r.Borders.Item(7).Weight=4; @{ls=$r.Borders.Item(7).LineStyle;w=$r.Borders.Item(7).Weight} }
 Observe 'Border.colorOnly' { $r=$s.Range('F4'); $r.Borders.Item(7).Color=255; @{ls=$r.Borders.Item(7).LineStyle;c=$r.Borders.Item(7).Color;w=$r.Borders.Item(7).Weight} }
 foreach($combo in @(@(-4115,1),@(-4115,4),@(-4118,-4138),@(-4118,4),@(-4119,2),@(-4119,-4138),@(-4119,1),@(13,2),@(13,-4138),@(4,2),@(5,2))){
   Observe "Border.combo.ls$($combo[0]).w$($combo[1])" { $r=$s.Range('H2'); $r.Clear()|Out-Null; $r.Borders.Item(7).LineStyle=$combo[0]; try{$r.Borders.Item(7).Weight=$combo[1]}catch{$e=$_.Exception.Message}; @{ls=$r.Borders.Item(7).LineStyle;w=$r.Borders.Item(7).Weight;err=$e} } }
 Observe 'Border.noLine.weight' { $r=$s.Range('J2'); @{w=$r.Borders.Item(7).Weight;c=$r.Borders.Item(7).Color;ci=$r.Borders.Item(7).ColorIndex;ls=$r.Borders.Item(7).LineStyle} }
 Observe 'Border.Count' { @{count=$s.Range('B2:C3').Borders.Count;cell=$s.Range('B2').Borders.Count} }
 Observe 'Border.range.weight.mixed' { $r=$s.Range('B2:C3'); @{w=$r.Borders.Weight;ls=$r.Borders.LineStyle;c=$r.Borders.Color} }
} finally { $b.Close($false) }
# ---- interior (cl5 3)
$b=Book
try {
 $s=$b.Worksheets.Item(1)
 Observe 'Interior.nofill' { $r=$s.Range('B2'); @{color=$r.Interior.Color;ci=$r.Interior.ColorIndex;pattern=$r.Interior.Pattern;pc=$r.Interior.PatternColor;pci=$r.Interior.PatternColorIndex} }
 Observe 'Interior.solid' { $r=$s.Range('B3'); $r.Interior.Color=255; @{color=$r.Interior.Color;ci=$r.Interior.ColorIndex;pattern=$r.Interior.Pattern;pc=$r.Interior.PatternColor;pci=$r.Interior.PatternColorIndex} }
 Observe 'Interior.rgb.nearest' { $r=$s.Range('B4'); $r.Interior.Color=0x123456; @{color=$r.Interior.Color;ci=$r.Interior.ColorIndex} }
 Observe 'Interior.patternAutomatic' { $r=$s.Range('B5'); $r.Interior.Pattern=-4105; @{p=$r.Interior.Pattern;color=$r.Interior.Color} }
 Observe 'Interior.gradient' { $r=$s.Range('B6'); $r.Interior.Pattern=4000; @{p=$r.Interior.Pattern} }
 Observe 'Interior.pattern.set' { $r=$s.Range('B7'); $r.Interior.Pattern=2; $r.Interior.PatternColor=255; @{p=$r.Interior.Pattern;pc=$r.Interior.PatternColor} }
 $s.Range('B9').Value2='x'
 Observe 'Style.hyperlink' { $r=$s.Range('B9'); $r.Font.Bold=$true; $r.Style='Hyperlink'; @{bold=$r.Font.Bold;color=$r.Font.Color;ul=$r.Font.Underline;style=$r.Style.Name} }
} finally { $b.Close($false) }
# ---- FormatConditions (dc-tqcvu)
$b=Book
try {
 $s=$b.Worksheets.Item(1); $s.Name='Sheet1'
 $s.Range('B2:B6').Value2=5
 $s.Range('D10').Select()
 Observe 'FC.cellValue.activeD10' { $fc=$s.Range('B2:B6').FormatConditions.Add(1,5,'=3'); @{f1=$fc.Formula1;op=$fc.Operator;type=$fc.Type;pri=$fc.Priority;count=$s.Range('B2:B6').FormatConditions.Count;applies=$fc.AppliesTo.Address()} }
 Observe 'FC.expression.activeD10' { $fc=$s.Range('B2:B6').FormatConditions.Add(2,$missing,'=B2>B3'); @{f1=$fc.Formula1;pri=$fc.Priority;type=$fc.Type} }
 Observe 'FC.expression.Operator' { $fc=$s.Range('B2:B6').FormatConditions.Item(2); $fc.Operator }
 Observe 'FC.cellValue.Formula2' { $fc=$s.Range('B2:B6').FormatConditions.Item(1); $fc.Formula2 }
 Observe 'FC.interior.beforeSet' { $fc=$s.Range('B2:B6').FormatConditions.Item(1); @{c=$fc.Interior.Color;ci=$fc.Interior.ColorIndex} }
 Observe 'FC.Item.countPlus1' { $s.Range('B2:B6').FormatConditions.Item(3) }
 Observe 'FC.Item.0' { $s.Range('B2:B6').FormatConditions.Item(0) }
 Observe 'FC.Add.99' { $s.Range('B2:B6').FormatConditions.Add(99,$missing,'=1') }
 Observe 'FC.Add.noOperator' { $s.Range('B2:B6').FormatConditions.Add(1) }
 Observe 'FC.Add.noOperatorFormula' { $s.Range('B2:B6').FormatConditions.Add(1,$missing,'=3') }
 Observe 'FC.Add.between.noF2' { $s.Range('B2:B6').FormatConditions.Add(1,1,'=1') }
 Observe 'FC.Count.otherRange' { @{d1=$s.Range('D1').FormatConditions.Count;b2=$s.Range('B2').FormatConditions.Count;all=$s.Cells.FormatConditions.Count} }
 Observe 'FC.font.bold.false' { $fc=$s.Range('B2:B6').FormatConditions.Item(1); $fc.Font.Bold=$false; @{b=$fc.Font.Bold} }
 Observe 'FC.priority.new' { $fc=$s.Range('B2:B6').FormatConditions.Add(2,$missing,'=1=1'); @{pri=$fc.Priority;count=$s.Cells.FormatConditions.Count} }
 foreach($p in 0,4,-1,99){ Observe "FC.priority.put$p" { $fc=$s.Range('B2:B6').FormatConditions.Item(1); $fc.Priority=$p; @{pri=$fc.Priority} } }
 Observe 'FC.AppliesTo' { $fc=$s.Range('B2:B6').FormatConditions.Item(1); $fc.AppliesTo.Address() }
 Observe 'FC.ModifyAppliesTo.otherSheet' { $t=$b.Worksheets.Add(); $fc=$s.Range('B2:B6').FormatConditions.Item(1); $fc.ModifyAppliesToRange($t.Range('A1:A2')); 'ok' }
 Observe 'FC.Delete.part' { $s.Range('B3:B4').FormatConditions.Delete(); @{count=$s.Cells.FormatConditions.Count;applies=$s.Cells.FormatConditions.Item(1).AppliesTo.Address()} }
 Observe 'FC.top10' { $fc=$s.Range('C2:C6').FormatConditions.AddTop10(); @{rank=$fc.Rank;tb=$fc.TopBottom;type=$fc.Type} }
} finally { $b.Close($false) }
# ---- Styles
$b=Book
try {
 Observe 'Styles.Count.new' { $b.Styles.Count }
 Observe 'Style.Normal' { $n=$b.Styles.Item('Normal'); @{h=$n.HorizontalAlignment;v=$n.VerticalAlignment;ind=$n.IndentLevel;o=$n.Orientation;wrap=$n.WrapText;shrink=$n.ShrinkToFit;locked=$n.Locked;fh=$n.FormulaHidden;val=$n.Value;nf=$n.NumberFormat} }
 Observe 'Style.Currency' { $n=$b.Styles.Item('Currency'); @{h=$n.HorizontalAlignment;nf=$n.NumberFormat;builtin=$n.BuiltIn} }
 $b.Worksheets.Item(1).Range('B2').Font.Bold=$true; $b.Worksheets.Item(1).Range('B2').Interior.Color=255; $b.Worksheets.Item(1).Range('B2').Select()
 Observe 'Styles.Add.X' { $x=$b.Styles.Add('X'); @{bold=$x.Font.Bold;fill=$x.Interior.Color;nf=$x.NumberFormat;value=$x.Value;h=$x.HorizontalAlignment;v=$x.VerticalAlignment;locked=$x.Locked;fh=$x.FormulaHidden;count=$b.Styles.Count} }
 Observe 'Styles.Add.dup' { $b.Styles.Add('X') }
 Observe 'Styles.Add.empty' { $b.Styles.Add('') }
 Observe 'Styles.nope' { $b.Styles.Item('nope') }
 Observe 'Style.Delete.inUse' { $b.Worksheets.Item(1).Range('C2').Style='X'; $b.Styles.Item('X').Delete(); @{style=$b.Worksheets.Item(1).Range('C2').Style.Name} }
 Observe 'Style.Delete.Normal' { $b.Styles.Item('Normal').Delete(); 'ok' }
} finally { $b.Close($false) }
# ---- Window
$b=Book
try {
 $w=$excel.ActiveWindow; $s=$b.Worksheets.Item(1)
 Observe 'Window.basic' { @{caption=$w.Caption;type=$w.Type;num=$w.WindowNumber;visible=$w.Visible;state=$w.WindowState;zoom=$w.Zoom;grid=$w.DisplayGridlines;scrollRow=$w.ScrollRow;scrollCol=$w.ScrollColumn;split=@($w.SplitRow,$w.SplitColumn);frozen=$w.FreezePanes} }
 Observe 'Window.Windows.2' { $excel.Windows.Item(2).Caption }
 Observe 'Window.Windows.name' { $excel.Windows.Item('nosuch').Caption }
 foreach($z in 9,401,10,400){ Observe "Window.Zoom$z" { $w.Zoom=$z; $w.Zoom } }
 $w.Zoom=100
 foreach($r in 0,1048577){ Observe "Window.ScrollRow$r" { $w.ScrollRow=$r; $w.ScrollRow } }
 foreach($c in 0,16385){ Observe "Window.ScrollColumn$c" { $w.ScrollColumn=$c; $w.ScrollColumn } }
 Observe 'Window.Freeze.A1' { $s.Activate(); $s.Range('A1').Select(); $w.FreezePanes=$true; @{f=$w.FreezePanes;sr=$w.SplitRow;sc=$w.SplitColumn} }
 $w.FreezePanes=$false
 Observe 'Window.Freeze.C3' { $w.ScrollRow=1; $s.Range('C3').Select(); $w.FreezePanes=$true; @{f=$w.FreezePanes;sr=$w.SplitRow;sc=$w.SplitColumn} }
 $w.FreezePanes=$false
 Observe 'Window.Freeze.scrolled' { $w.ScrollRow=5; $s.Range('C8').Select(); $w.FreezePanes=$true; @{f=$w.FreezePanes;sr=$w.SplitRow;sc=$w.SplitColumn;scrollRow=$w.ScrollRow} }
 $w.FreezePanes=$false
 Observe 'Window.Split' { $w.ScrollRow=1; $s.Range('C3').Select(); $w.Split=$true; @{split=$w.Split;sr=$w.SplitRow;sc=$w.SplitColumn;f=$w.FreezePanes} }
 Observe 'Window.Caption.empty' { $w.Caption=''; $w.Caption }
 Observe 'Window.Caption.number' { $w.Caption=5; $w.Caption }
 Observe 'Window.Caption.empty2' { $w.Caption=[Type]::Missing; $w.Caption }
 Observe 'Window.Caption.set' { $w.Caption='abc'; @{c=$w.Caption} }
 Observe 'Window.Caption.reset' { $w.Caption=$null; $w.Caption }
 Observe 'Window.WindowState' { $w.WindowState }
 Observe 'Window.chartsheet' { $c=$b.Charts.Add(); @{sel=$excel.Selection -eq $null;ac=$(try{$excel.ActiveCell.Address()}catch{'err '+$_.Exception.Message});selType=$(try{$excel.Selection.GetType().Name}catch{'err'})} }
} finally { $b.Close($false) }
