param($OwnedExcel,[string]$Request,[string]$Result)
# Pivot observations (ported from the UI round 28 job, chart kinds removed): layout, refresh, round trip
# and the measurement kinds m1..m4 added below. Answers are data: every Excel
# refusal is retained as an observed error. No workbook from outside this job is opened except the
# job's own saved outputs. Runs under the session host; returns, never quits Excel.
$ErrorActionPreference='Stop'
$excel=$OwnedExcel
$req=Get-Content -LiteralPath $Request -Raw -Encoding utf8|ConvertFrom-Json
foreach($k in 'scenario','input','chart_id'){if(-not $req.PSObject.Properties[$k]){$req|Add-Member -NotePropertyName $k -NotePropertyValue $null}}
. (Join-Path $PSScriptRoot 'hidden-window.ps1')
Start-HiddenExcel $excel
$visibility=$null
$security=$excel.AutomationSecurity
$dir=[string]$req.directory
$errors=[Collections.Generic.List[object]]::new()
$obs=[Collections.Generic.List[object]]::new()
$fatal=$null;$book=$null;$script:lightCells=$false;$script:fmtCells=$false;$script:rowInfo=@{};$script:colInfo=@{}
function Try-Step([string]$Name,[scriptblock]$Do){
    try{ & $Do; $obs.Add(@{step=$Name;success=$true;utc=[DateTime]::UtcNow.ToString('O')}); return $true }
    catch{ $errors.Add(@{step=$Name;message=$_.Exception.Message;hresult=$_.Exception.HResult;detail=$_.Exception.ToString()}); $obs.Add(@{step=$Name;success=$false;message=$_.Exception.Message;utc=[DateTime]::UtcNow.ToString('O')}); return $false }
}
function Enc($v){
    if($null -eq $v){return @{type='empty'}}
    if($v -is [double]){return @{type='double';value=$v;bits=('{0:x16}' -f [BitConverter]::DoubleToInt64Bits($v))}}
    if($v -is [bool]){return @{type='bool';value=$v}}
    if($v -is [int]){return @{type='error-or-int';value=$v}}
    if($v -is [datetime]){return @{type='date';value=$v.ToOADate()}}
    return @{type=$v.GetType().Name;value=[string]$v}
}
function Call($Object,[string]$Name,[object[]]$Arguments=$null){return ,[System.__ComObject].InvokeMember($Name,[Reflection.BindingFlags]::InvokeMethod -bor [Reflection.BindingFlags]::GetProperty,$null,$Object,$Arguments)}
function Items($c){$l=[Collections.Generic.List[object]]::new();if($null -eq $c){return ,$l};$n=[int]$c.Count;for($i=1;$i -le $n;$i++){$l.Add($c.Item($i))};return ,$l}
function Addr($Object,[string]$Name){try{$r=[System.__ComObject].InvokeMember($Name,[Reflection.BindingFlags]::GetProperty,$null,$Object,$null);if($null -eq $r){return $null};return [string]$r.Address()}catch{return @{unread=$_.InnerException.Message}}}
function Prop($Object,[string]$Name){try{return ,[System.__ComObject].InvokeMember($Name,[Reflection.BindingFlags]::GetProperty,$null,$Object,$null)}catch{return @{unread=$_.InnerException.Message}}}
function Fmt-Read($c){
    $d=$c.DisplayFormat
    $b=[ordered]@{}
    foreach($e in @(@('left',7),@('top',8),@('bottom',9),@('right',10))){try{$bd=$d.Borders.Item($e[1]);$b[$e[0]]=('{0}:{1}:{2}' -f $bd.LineStyle,$bd.Weight,$bd.Color)}catch{$b[$e[0]]='unread'}}
    $o=[ordered]@{address=[string]$c.Address($false,$false);value=(Enc $c.Value2);text=[string]$c.Text;has_formula=[bool]$c.HasFormula;number_format=[string]$d.NumberFormat;font_name=[string]$d.Font.Name;font_size=$d.Font.Size;bold=$d.Font.Bold;italic=$d.Font.Italic;underline=$d.Font.Underline;font_color=$d.Font.Color;fill=$d.Interior.Color;pattern=$d.Interior.Pattern;borders=$b;halign=$d.HorizontalAlignment;indent=$d.IndentLevel;column_width=$c.ColumnWidth;row_height=$c.RowHeight}
    try{$pc=$c.PivotCell;$o.pivot_cell_type=[int]$pc.PivotCellType}catch{$o.pivot_cell_type=$null}
    return $o
}
function Cell-Read($c){
    if($script:fmtCells){return (Fmt-Read $c)}
    if($script:lightCells){return [ordered]@{address=[string]$c.Address($false,$false);value=(Enc $c.Value2);has_formula=[bool]$c.HasFormula}}
    $rowKey=[int]$c.Row;$colKey=[int]$c.Column
    if(-not $script:rowInfo.ContainsKey($rowKey)){$script:rowInfo[$rowKey]=@{row_height=$c.RowHeight;row_hidden=[bool]$c.EntireRow.Hidden}}
    if(-not $script:colInfo.ContainsKey($colKey)){$script:colInfo[$colKey]=@{column_width=$c.ColumnWidth;column_hidden=[bool]$c.EntireColumn.Hidden}}
    $o=[ordered]@{address=[string]$c.Address($false,$false);value=(Enc $c.Value2);text=[string]$c.Text;has_formula=[bool]$c.HasFormula;number_format=[string]$c.NumberFormat;style=$(try{[string]$c.Style.Name}catch{$null});row_height=$script:rowInfo[$rowKey].row_height;row_hidden=$script:rowInfo[$rowKey].row_hidden;column_width=$script:colInfo[$colKey].column_width;column_hidden=$script:colInfo[$colKey].column_hidden;merged=[bool]$c.MergeCells;interior_color=$c.Interior.Color;bold=$c.Font.Bold}
    if($o.merged){$o.merge_area=[string]$c.MergeArea.Address($false,$false)}
    try{$pc=$c.PivotCell;$o.pivot_cell_type=[int]$pc.PivotCellType}catch{$o.pivot_cell_type=$null}
    return $o
}
function Range-Read($r){$script:rowInfo=@{};$script:colInfo=@{};$rows=[Collections.Generic.List[object]]::new();foreach($c in $r.Cells){$rows.Add((Cell-Read $c))};return ,$rows.ToArray()}
function Field-Read($f){
    $o=[ordered]@{name=[string]$f.Name;source=(Prop $f 'SourceName');orientation=(Prop $f 'Orientation');position=(Prop $f 'Position');function=(Prop $f 'Function');layout_form=(Prop $f 'LayoutForm');compact=(Prop $f 'LayoutCompactRow');subtotal_location=(Prop $f 'LayoutSubtotalLocation');layout_blank=(Prop $f 'LayoutBlankLine');repeat_labels=(Prop $f 'RepeatLabels');auto_sort_order=(Prop $f 'AutoSortOrder');auto_sort_field=(Prop $f 'AutoSortField');calculation=(Prop $f 'Calculation');base_field=(Prop $f 'BaseField');base_item=(Prop $f 'BaseItem');number_format=(Prop $f 'NumberFormat');show_all_items=(Prop $f 'ShowAllItems');subtotals=(Prop $f 'Subtotals')}
    if($script:lightCells){return $o}
    try{$items=[Collections.Generic.List[object]]::new();foreach($i in (Items (Call $f 'PivotItems'))){$items.Add(@{name=[string]$i.Name;caption=[string]$i.Caption;position=(Prop $i 'Position');visible=(Prop $i 'Visible');record_count=(Prop $i 'RecordCount')})};$o.items=$items.ToArray()}catch{$o.items=@{unread=$_.Exception.Message}}
    return $o
}
function Pivot-Read($p){
    $o=[ordered]@{sheet=$(try{[string]$p.Parent.Name}catch{$null});name=[string]$p.Name;version=(Prop $p 'Version');source=(Prop $p 'SourceData');table_range1=(Addr $p 'TableRange1');table_range2=(Addr $p 'TableRange2');row_range=(Addr $p 'RowRange');column_range=(Addr $p 'ColumnRange');data_body_range=(Addr $p 'DataBodyRange');page_range=(Addr $p 'PageRange');column_grand=(Prop $p 'ColumnGrand');row_grand=(Prop $p 'RowGrand');in_grid_drop_zones=(Prop $p 'InGridDropZones');has_auto_format=(Prop $p 'HasAutoFormat');preserve_formatting=(Prop $p 'PreserveFormatting');layout_row_default=(Prop $p 'LayoutRowDefault');compact_layout_row_header=(Prop $p 'CompactLayoutRowHeader');display_error_string=(Prop $p 'DisplayErrorString');error_string=(Prop $p 'ErrorString');null_string=(Prop $p 'NullString');missing_items_limit=(Prop (Call $p 'PivotCache') 'MissingItemsLimit');cache_records=(Prop (Call $p 'PivotCache') 'RecordCount');cache_index=(Prop $p 'CacheIndex');refresh_date=$(try{[string]$p.RefreshDate}catch{$null});table_style=(Prop $p 'TableStyle2');show_row_headers=(Prop $p 'ShowTableStyleRowHeaders');show_column_headers=(Prop $p 'ShowTableStyleColumnHeaders');show_row_stripes=(Prop $p 'ShowTableStyleRowStripes');show_column_stripes=(Prop $p 'ShowTableStyleColumnStripes');show_last_column=(Prop $p 'ShowTableStyleLastColumn');grand_total_name=(Prop $p 'GrandTotalName');column_header=(Prop $p 'CompactLayoutColumnHeader')}
    foreach($axis in 'RowFields','ColumnFields','PageFields','DataFields'){
        try{$fields=[Collections.Generic.List[object]]::new();foreach($f in (Items (Call $p $axis))){$fields.Add((Field-Read $f))};$o[$axis]=$fields.ToArray()}catch{$o[$axis]=@{unread=$_.Exception.Message}}
    }
    try{$o.cells=Range-Read $p.TableRange2}catch{$o.cells=@{unread=$_.Exception.Message}}
    return $o
}
function Pivots-Read([string]$Stage,$Extra=$null){
    Assert-HiddenExcel $Stage
    $all=[Collections.Generic.List[object]]::new()
    foreach($ws in $book.Worksheets){foreach($p in (Items (Call $ws 'PivotTables'))){$all.Add((Pivot-Read $p))}}
    $rec=[ordered]@{stage=$Stage;utc=[DateTime]::UtcNow.ToString('O');pivots=$all.ToArray()}
    if($Extra){$rec.extra=& $Extra}
    $obs.Add($rec)
}
function Build-Source($Book){
    $sh=$Book.Worksheets.Item(1);$sh.Name='Source'
    $h=[object[,]]::new(37,5);$names='Region','Product','Month','Amount','Units';for($i=0;$i -lt 5;$i++){$h[0,$i]=$names[$i]}
    $regions='North','South','East','West';$products='Alpha','Beta','Gamma'
    $start=[datetime]'2024-01-15'
    for($i=0;$i -lt 36;$i++){$h[($i+1),0]=$regions[$i%4];$h[($i+1),1]=$products[$i%3];$h[($i+1),2]=$start.AddMonths($i).ToOADate();$h[($i+1),3]=[double](100+$i*7.25);$h[($i+1),4]=[double](1+$i%5)}
    for($j=0;$j -lt 5;$j++){$h[5,$j]=$h[4,$j]}          # row 6 duplicates row 5
    $h[9,0]=$null;$h[13,3]=$null;$h[17,3]=[double]-75.5;$h[21,3]=[double]0;$h[25,3]='n/a'
    $sh.Range('A1:E37').Value2=$h
    $sh.Range('C2:C37').NumberFormat='yyyy-mm-dd';$sh.Range('D2:D37').NumberFormat='#,##0.00'
    $lo=$sh.ListObjects.Add(1,$sh.Range('A1:E37'),$null,1);$lo.Name='SalesTable';$lo.TableStyle='TableStyleMedium2'
    [void]$Book.Names.Add('SalesRange','=Source!$A$1:$E$37')
    return $sh
}
function New-Pivot($Book,[string]$Sheet,[string]$Source,[string]$Name,[string]$At='A3'){
    $ws=$Book.Worksheets.Add([Type]::Missing,$Book.Worksheets.Item($Book.Worksheets.Count));$ws.Name=$Sheet
    $cache=$Book.PivotCaches().Create(1,$Source,8)
    return $cache.CreatePivotTable($ws.Range($At),$Name,$true,8)
}
function Add-Data($p,[string]$Field,[string]$Caption,[int]$Fn){[void]$p.AddDataField($p.PivotFields($Field),$Caption,$Fn)}
function Row($p,[string]$F,[int]$Pos=1){$x=$p.PivotFields($F);$x.Orientation=1;$x.Position=$Pos}
function Col($p,[string]$F,[int]$Pos=1){$x=$p.PivotFields($F);$x.Orientation=2;$x.Position=$Pos}
function Save([string]$Name){$book.SaveAs((Join-Path $dir $Name),51);$obs.Add(@{step="saved $Name";path=(Join-Path $dir $Name);success=$true})}
$RangeSource='Source!R1C1:R37C5'
function Stage-Pivot([string]$Stage,$p,$Extra=$null){
    Assert-HiddenExcel $Stage
    $rec=[ordered]@{stage=$Stage;utc=[DateTime]::UtcNow.ToString('O');pivots=@(,(Pivot-Read $p))}
    if($Extra){$rec.extra=& $Extra}
    $obs.Add($rec)
}
$script:caseNo=0
function Case([string]$Id,[string]$Source,[scriptblock]$Build){
    [void](Try-Step "case-$Id" {
        $script:caseNo++
        $ws=$book.Worksheets.Add([Type]::Missing,$book.Worksheets.Item($book.Worksheets.Count));$ws.Name='C'+$script:caseNo
        $cache=$book.PivotCaches().Create(1,$Source,8)
        $p=$cache.CreatePivotTable($ws.Range('A3'),('pvt_'+$Id.Replace('-','_')),$true,8)
        & $Build $p
        Stage-Pivot $Id $p
    })
}
function Set-Calc($p,[int]$Index,[int]$Calc,$Base,$Item){
    $df=(Items (Call $p 'DataFields'))[$Index-1];$df.Calculation=$Calc
    if($Base){$df.BaseField=$Base};if($Item){$df.BaseItem=$Item}
}
function Map($Collection,[scriptblock]$Do){$r=[Collections.Generic.List[object]]::new();foreach($i in (Items $Collection)){$r.Add((& $Do $i))};return ,$r.ToArray()}
function Set-Member($Object,[string]$Name,$Value){[void][System.__ComObject].InvokeMember($Name,[Reflection.BindingFlags]::SetProperty,$null,$Object,@(,$Value))}
function Build-M1Sources($Book){
    $E=[Collections.Generic.List[object]]::new()
    # Cat, Key, Val; strings starting with '=' are written as formulas (error values).
    foreach($r in @(@('x','a',10),@('x','a',20),@('y','b',5),@('y','b','=NA()'),@('y','=NA()',7),@('z','=1/0','=1/0'),@('z',$null,3),@('w',' ','abc'),@('w','',4),@('v',$true,$true),@('v',1,2),
        @('u','1',$null),@('u',1,6),@('t',0,1.5),@('t',-1,2.5),@('s',45000,45000),@('s','A',3),@('s','a',4),@('s','A ',5),@('q','k0',1),@('q','k1',2),@('q','k2',3),@('q','k0',4),@('q','k1',5),@('q','k2',6),@('q','k0',7),@('q','k1',8),@('q','k2',9),@('q',$null,$null),@('q','=NA()',$null))){$E.Add($r)}
    $ws=$Book.Worksheets.Add([Type]::Missing,$Book.Worksheets.Item($Book.Worksheets.Count));$ws.Name='Errs'
    $ws.Range('A1').Value2='Cat';$ws.Range('B1').Value2='Key';$ws.Range('C1').Value2='Val'
    $row=2
    foreach($r in $E){
        for($c=0;$c -lt 3;$c++){
            $v=$r[$c];$cell=$ws.Cells.Item($row,$c+1)
            if($null -eq $v){continue}
            if($v -is [string] -and $v.StartsWith('=')){$cell.Formula=$v}
            elseif($v -is [string] -and $v -eq ''){$cell.Formula='=""'}
            elseif($v -is [string]){$cell.NumberFormat='@';$cell.Value2=$v}
            else{$cell.Value2=$v}
        }
        $row++
    }
    $script:errsRows=$row-1
    $ws.Range('B16:B16').NumberFormat='yyyy-mm-dd';$ws.Range('B20:B20').NumberFormat='yyyy-mm-dd'
    $groups=@(@('G1',@(0.1,0.1,0.1,0.1,0.1,0.1,0.1,0.1,0.1,0.1)),@('G2',@(1e16,1.0,-1e16)),@('G3',@(1.0,1e16,-1e16)),@('G4',@(0.1,0.2,0.3)),@('G5',@(1e308,1e308,-1e308)),@('G6',@(1000000000000000.3,-1e15,0.7)),
        @('G7',@(1e-10,2e-10,3e-10,4e-10,5e-10)),@('G8',@(1.1,2.2,3.3)),@('G9',@(1e154,1e154)),@('G10',@(16777216.0,1.0,1.0)),@('G11',@(1.0,-1.0,1.0,-1.0)),@('G12',@(7.0)),@('G13',@(5e-324,5e-324)))
    $pw=$Book.Worksheets.Add([Type]::Missing,$Book.Worksheets.Item($Book.Worksheets.Count));$pw.Name='Prec'
    $pw.Range('A1').Value2='Grp';$pw.Range('B1').Value2='Val';$pw.Range('C1').Value2='Ord'
    $row=2
    foreach($g in $groups){$k=0;foreach($v in $g[1]){$k++;$pw.Cells.Item($row,1).Value2=$g[0];$pw.Cells.Item($row,2).Value2=[double]$v;$pw.Cells.Item($row,3).Value2=[double]$k;$row++}}
    $script:precRows=$row-1
}

function Pivot-Set($Book){
    $spec=@(
     @('one-row-sum',{param($p)Row $p 'Region';Add-Data $p 'Amount' 'Sum Amount' -4157}),
     @('two-row-compact',{param($p)Row $p 'Region' 1;Row $p 'Product' 2;Add-Data $p 'Amount' 'Sum Amount' -4157;$p.RowAxisLayout(0)}),
     @('two-row-outline',{param($p)Row $p 'Region' 1;Row $p 'Product' 2;Add-Data $p 'Amount' 'Sum Amount' -4157;$p.RowAxisLayout(2)}),
     @('two-row-tabular',{param($p)Row $p 'Region' 1;Row $p 'Product' 2;Add-Data $p 'Amount' 'Sum Amount' -4157;$p.RowAxisLayout(1)}),
     @('row-col',{param($p)Row $p 'Region';Col $p 'Product';Add-Data $p 'Amount' 'Sum Amount' -4157}),
     @('row-col-dates',{param($p)Row $p 'Region';Col $p 'Month';Add-Data $p 'Amount' 'Sum Amount' -4157}),
     @('two-data',{param($p)Row $p 'Region';Add-Data $p 'Amount' 'Sum Amount' -4157;Add-Data $p 'Units' 'Sum Units' -4157}),
     @('report-filter',{param($p)$x=$p.PivotFields('Product');$x.Orientation=3;Row $p 'Region';Add-Data $p 'Amount' 'Sum Amount' -4157}),
     @('count',{param($p)Row $p 'Region';Add-Data $p 'Amount' 'Count Amount' -4112}),
     @('average',{param($p)Row $p 'Region';Add-Data $p 'Amount' 'Average Amount' -4106}),
     @('min',{param($p)Row $p 'Region';Add-Data $p 'Amount' 'Min Amount' -4139}),
     @('max',{param($p)Row $p 'Region';Add-Data $p 'Amount' 'Max Amount' -4136}),
     @('grand-off',{param($p)Row $p 'Region';Col $p 'Product';Add-Data $p 'Amount' 'Sum Amount' -4157;$p.ColumnGrand=$false;$p.RowGrand=$false}),
     @('subtotal-top',{param($p)Row $p 'Region' 1;Row $p 'Product' 2;Add-Data $p 'Amount' 'Sum Amount' -4157;$p.RowAxisLayout(2);$p.SubtotalLocation(1)}),
     @('subtotal-bottom',{param($p)Row $p 'Region' 1;Row $p 'Product' 2;Add-Data $p 'Amount' 'Sum Amount' -4157;$p.RowAxisLayout(2);$p.SubtotalLocation(2)}),
     @('repeat-on',{param($p)Row $p 'Region' 1;Row $p 'Product' 2;Add-Data $p 'Amount' 'Sum Amount' -4157;$p.RowAxisLayout(1);$p.RepeatAllLabels(2)}),
     @('repeat-off',{param($p)Row $p 'Region' 1;Row $p 'Product' 2;Add-Data $p 'Amount' 'Sum Amount' -4157;$p.RowAxisLayout(1);$p.RepeatAllLabels(1)}),
     @('sort-ascending',{param($p)Row $p 'Region';Add-Data $p 'Amount' 'Sum Amount' -4157;$p.PivotFields('Region').AutoSort(1,'Region')}),
     @('sort-descending',{param($p)Row $p 'Region';Add-Data $p 'Amount' 'Sum Amount' -4157;$p.PivotFields('Region').AutoSort(2,'Region')}),
     @('sort-manual',{param($p)Row $p 'Region';Add-Data $p 'Amount' 'Sum Amount' -4157;$p.PivotFields('Region').AutoSort(-4135,'Region');$p.PivotFields('Region').PivotItems('West').Position=1;$p.PivotFields('Region').PivotItems('East').Position=3})
    )
    $n=0
    foreach($s in $spec){
        $n++;$id=$s[0];$fn=$s[1]
        [void](Try-Step ("pivot-$id") { $p=New-Pivot $Book ('P'+$n) $RangeSource ('pvt_'+$id.Replace('-','_')); & $fn $p })
    }
    [void](Try-Step 'pivot-defined-name-source' { $p=New-Pivot $Book 'PName' 'SalesRange' 'pvt_named';Row $p 'Region';Add-Data $p 'Amount' 'Sum Amount' -4157 })
    [void](Try-Step 'pivot-table-source' { $p=New-Pivot $Book 'PTable' 'SalesTable' 'pvt_table';Row $p 'Region';Add-Data $p 'Amount' 'Sum Amount' -4157 })
}
try{
    $excel.DisplayAlerts=$false;$excel.EnableEvents=$false;$excel.ScreenUpdating=$false
    $book=$excel.Workbooks.Add(-4167)
    Assert-HiddenExcel 'after-create'
    switch([string]$req.kind){
     'pivots' {
        $excel.Calculation=-4105
        [void](Build-Source $book)
        Pivot-Set $book
        Pivots-Read 'built'
        Save 'state1-built.xlsx'
        $src=$book.Worksheets.Item('Source')
        $src.Range('D3').Value2=[double]1234.5
        $script:lightCells=$true;Pivots-Read 'after-source-edit-no-refresh';$script:lightCells=$false
        foreach($ws in $book.Worksheets){foreach($p in (Items (Call $ws 'PivotTables'))){[void](Try-Step ("refresh-"+$p.Name) {[void]$p.RefreshTable()})}}
        Pivots-Read 'after-refresh'
        Save 'state2-refreshed.xlsx'
     }
     'refresh' {
        $excel.Calculation=-4105
        $src=Build-Source $book
        $lo=$src.ListObjects.Item('SalesTable')
        $p=New-Pivot $book 'P' $RangeSource 'pvt_range';Row $p 'Region' 1;Row $p 'Product' 2;Add-Data $p 'Amount' 'Sum Amount' -4157;$p.RowAxisLayout(1)
        $p=$book.Worksheets.Item('P').PivotTables('pvt_range')
        $q=$null
        $block={ @{block=(Range-Read $book.Worksheets.Item('P').Range('A1:F40'))} }
        $scenario=[string]$req.scenario
        function All-Refresh([string]$Tag){foreach($ws in $book.Worksheets){foreach($x in (Items (Call $ws 'PivotTables'))){[void](Try-Step ("$Tag "+$x.Name) {$r=$x.RefreshTable();$obs.Add(@{refresh=$x.Name;returned=$r})})}}}
        switch($scenario){
         'source-sort' {
            $q=New-Pivot $book 'Q' 'SalesTable' 'pvt_table';Row $q 'Region';Add-Data $q 'Amount' 'Sum Amount' -4157
            Pivots-Read 'before' ;Save 'before.xlsx'
            $lo.Sort.SortFields.Clear();[void]$lo.Sort.SortFields.Add($src.Range('A2:A37'),0,2);$lo.Sort.Header=1;$lo.Sort.Apply()
            Pivots-Read 'after-change-no-refresh';All-Refresh 'refresh';Pivots-Read 'after-refresh'
         }
         'add-category' {
            Pivots-Read 'before' ;Save 'before.xlsx'
            $src.Range('B3').Value2='Zeta';$src.Range('A4').Value2='Aardvark'
            Pivots-Read 'after-change-no-refresh';All-Refresh 'refresh';Pivots-Read 'after-refresh'
         }
         'remove-category' {
            Pivots-Read 'before' ;Save 'before.xlsx'
            foreach($c in $src.Range('B2:B37').Cells){if($c.Value2 -eq 'Gamma'){$c.Value2='Alpha'}}
            Pivots-Read 'after-change-no-refresh';All-Refresh 'refresh';Pivots-Read 'after-refresh'
            $p=$book.Worksheets.Item('P').PivotTables('pvt_range')
            [void](Try-Step 'set-missing-items-none' {(Call $p 'PivotCache').MissingItemsLimit=0});All-Refresh 'refresh-limit-0';Pivots-Read 'after-missing-limit-0-refresh'
         }
         'add-row' {
            $q=New-Pivot $book 'Q' 'SalesTable' 'pvt_table';Row $q 'Region';Add-Data $q 'Amount' 'Sum Amount' -4157
            Pivots-Read 'before' ;Save 'before.xlsx'
            $n=$lo.ListRows.Add();$v=$n.Range;$v.Cells.Item(1,1).Value2='NewRegion';$v.Cells.Item(1,2).Value2='Alpha';$v.Cells.Item(1,3).Value2=45800.0;$v.Cells.Item(1,4).Value2=[double]55.25;$v.Cells.Item(1,5).Value2=[double]3
            Pivots-Read 'after-change-no-refresh';All-Refresh 'refresh';Pivots-Read 'after-refresh'
         }
         'blank-and-error' {
            Pivots-Read 'before' ;Save 'before.xlsx'
            $src.Range('A3').ClearContents()|Out-Null;$src.Range('D5').Formula='=NA()'
            Pivots-Read 'after-change-no-refresh';All-Refresh 'refresh';Pivots-Read 'after-refresh'
         }
         'output-edit' {
            Pivots-Read 'before' $block;Save 'before.xlsx'
            $body=$p.DataBodyRange.Cells.Item(2,1)
            [void](Try-Step 'edit-data-body-cell' {$body.Value2=[double]999})
            Pivots-Read 'after-data-edit' $block
            [void](Try-Step 'clear-data-body-cell' {$body.ClearContents()|Out-Null})
            [void](Try-Step 'rename-row-label-item' {$p.RowRange.Cells.Item(2,1).Value2='RenamedNorth'})
            Pivots-Read 'after-label-edit' $block;All-Refresh 'refresh';Pivots-Read 'after-refresh' $block
         }
         'overlap-nonblank' {
            $sh=$book.Worksheets.Item('P');$end=$p.TableRange2.Row+$p.TableRange2.Rows.Count
            $sh.Cells.Item($end+1,1).Value2='SENTINEL BELOW';$sh.Cells.Item($end+1,3).Value2=[double]777
            Pivots-Read 'before' $block;Save 'before.xlsx'
            foreach($i in 3..12){$src.Cells.Item($i,2).Value2='New'+$i}
            Pivots-Read 'after-change-no-refresh' $block
            All-Refresh 'refresh-alerts-off';Pivots-Read 'after-refresh' $block
         }
         'overlap-pivot' {
            $sh=$book.Worksheets.Item('P');$end=$p.TableRange2.Row+$p.TableRange2.Rows.Count
            $cache=$book.PivotCaches().Create(1,$RangeSource,8);$q=$cache.CreatePivotTable($sh.Cells.Item($end+2,1),'pvt_below',$true,8);Row $q 'Region';Add-Data $q 'Units' 'Sum Units' -4157
            Pivots-Read 'before' $block;Save 'before.xlsx'
            foreach($i in 3..12){$src.Cells.Item($i,2).Value2='New'+$i}
            Pivots-Read 'after-change-no-refresh' $block
            All-Refresh 'refresh-overlap';Pivots-Read 'after-refresh' $block
         }
         'manual-pending' {
            $src.Range('H1').Value2=[double]2;$src.Range('D3').Formula='=H1*100'
            $excel.Calculation=-4135;$excel.Calculate();$p.RefreshTable()|Out-Null
            Pivots-Read 'before' ;Save 'before.xlsx'
            $src.Range('H1').Value2=[double]5
            $obs.Add(@{d3_pending_value=(Enc $src.Range('D3').Value2);calculation_state=[int]$excel.CalculationState})
            All-Refresh 'refresh-before-calculate';Pivots-Read 'after-refresh-before-calculate'
            $excel.Calculate();$obs.Add(@{d3_after_calculate=(Enc $src.Range('D3').Value2)})
            Pivots-Read 'after-calculate-no-refresh';All-Refresh 'refresh-after-calculate';Pivots-Read 'after-refresh-after-calculate'
         }
         'format-shrink' {
            $sh=$book.Worksheets.Item('P')
            $p.PreserveFormatting=$true
            $p.DataBodyRange.Cells.Item(2,1).Interior.Color=65535;$p.DataBodyRange.Cells.Item(5,1).Font.Bold=$true
            $p.RowRange.Cells.Item(3,1).Interior.Color=255
            $end=$p.TableRange2.Row+$p.TableRange2.Rows.Count
            $sh.Cells.Item($end+1,1).Interior.Color=65280;$sh.Cells.Item($end+1,1).Value2='OUTSIDE FORMATTED'
            $sh.Cells.Item($end-3,6).Interior.Color=16711680
            Pivots-Read 'before' $block;Save 'before.xlsx'
            foreach($c in $src.Range('B2:B37').Cells){if($c.Value2 -eq 'Gamma'){$c.Value2='Alpha'}}
            foreach($c in $src.Range('A2:A37').Cells){if($c.Value2 -eq 'West'){$c.Value2='East'}}
            [void](Try-Step 'missing-none' {(Call $p 'PivotCache').MissingItemsLimit=0})
            All-Refresh 'refresh-shrink';Pivots-Read 'after-shrink-refresh' $block
         }
         'grow-shift' {
            $sh=$book.Worksheets.Item('P')
            $sh.Range('A100').Value2='FAR SENTINEL';$sh.Range('H5').Value2='RIGHT SENTINEL';$sh.Rows.Item(60).RowHeight=33
            Pivots-Read 'before' {@{a100=(Enc $sh.Range('A100').Value2);h5=(Enc $sh.Range('H5').Value2);row60=$sh.Rows.Item(60).RowHeight;used=[string]$sh.UsedRange.Address()}};Save 'before.xlsx'
            foreach($i in 3..14){$src.Cells.Item($i,2).Value2='New'+$i}
            All-Refresh 'refresh-grow';Pivots-Read 'after-grow' {@{a100=(Enc $sh.Range('A100').Value2);h5=(Enc $sh.Range('H5').Value2);row60=$sh.Rows.Item(60).RowHeight;used=[string]$sh.UsedRange.Address()}}
         }
     default {throw "unknown scenario $scenario"}
        }
        Save 'after.xlsx'
     }
     'roundtrip' {
        $book.Close($false);$book=$null
        $input=[string]$req.input
        $book=$excel.Workbooks.Open($input,0,$false)
        $excel.Calculation=-4105
        $obs.Add(@{opened=$input;repair_mode_file=[string]$book.Name;has_pivots=(Call $book.Worksheets.Item('P1') 'PivotTables').Count})
        Pivots-Read 'opened'
        Save 'excel-resave-opened.xlsx'
        foreach($ws in $book.Worksheets){foreach($x in (Items (Call $ws 'PivotTables'))){[void](Try-Step ("refresh "+$x.Name) {[void]$x.RefreshTable()})}}
        Pivots-Read 'after-refresh'
        Save 'excel-resave-refreshed.xlsx'
     }
         'm1' {
        # M1 calculation: aggregations, Show Values As, calculated fields/items, grouping, Values on rows,
        # subtotals, blank lines, items with no data, sort by value, error/blank items, missing items, precision.
        $excel.Calculation=-4105
        [void](Build-Source $book)
        Build-M1Sources $book
        $SRC='Source!R1C1:R37C5'
        $fns=@(@('Sum',-4157),@('Count',-4112),@('CountNums',-4113),@('Average',-4106),@('Max',-4136),@('Min',-4139),@('Product',-4149),@('StDev',-4155),@('StDevP',-4156),@('Var',-4164),@('VarP',-4165))
        Case 'agg-all-amount' $SRC { param($p) Row $p 'Region'; foreach($f in $fns){Add-Data $p 'Amount' ($f[0]+' Amt') $f[1]} }
        Case 'agg-all-units' $SRC { param($p) Row $p 'Product'; Col $p 'Region'; foreach($f in $fns){Add-Data $p 'Units' ($f[0]+' Unt') $f[1]} }
        Case 'agg-text-field' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Product' 'Count Product' -4112; Add-Data $p 'Product' 'Sum Product' -4157; Add-Data $p 'Month' 'Max Month' -4136; Add-Data $p 'Month' 'Count Month' -4112 }
        Case 'agg-two-level-columns' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'StDev Amt' -4155; Add-Data $p 'Amount' 'Product Amt' -4149 }
        $showAs=@(@('percent-of-total',8,$null,$null),@('percent-of-column',7,$null,$null),@('percent-of-row',6,$null,$null),@('index',9,$null,$null),@('percent-of-parent-row',10,$null,$null),@('percent-of-parent-column',11,$null,$null),
            @('difference-previous',2,'Region','(previous)'),@('difference-next',2,'Region','(next)'),@('difference-east',2,'Region','East'),@('percentof-east',3,'Region','East'),@('percentof-previous',3,'Region','(previous)'),
            @('percent-difference-previous',4,'Region','(previous)'),@('percent-difference-east',4,'Region','East'),@('running-total',5,'Region',$null),@('percent-running-total',13,'Region',$null),
            @('rank-ascending',14,'Region',$null),@('rank-descending',15,'Region',$null),@('difference-product-alpha',2,'Product','Alpha'),@('running-total-product',5,'Product',$null))
        foreach($row in $showAs){ $sa=$row; Case ('show-'+$sa[0]) $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; Set-Calc $p 1 $sa[1] $sa[2] $sa[3]  } }
        Case 'show-percent-of-parent' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; Set-Calc $p 1 12 'Region' $null; $p.RowAxisLayout(1) }
        Case 'show-percent-of-parent-product' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; Set-Calc $p 1 12 'Product' $null; $p.RowAxisLayout(1) }
        Case 'show-two-data-fields' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Pct Amount' -4157; Add-Data $p 'Units' 'Run Units' -4157; Set-Calc $p 1 8 $null $null; Set-Calc $p 2 5 'Region' $null }
        Case 'show-count-average' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Avg Amount' -4106; Set-Calc $p 1 7 $null $null }
        Case 'calc-field-ratio' $SRC { param($p) [void]$p.CalculatedFields().Add('PricePerUnit','=Amount/Units',$true); Row $p 'Region'; Add-Data $p 'PricePerUnit' 'Sum Price' -4157; Add-Data $p 'Amount' 'Sum Amount' -4157 }
        Case 'calc-field-if' $SRC { param($p) [void]$p.CalculatedFields().Add('Big','=IF(Units>2,Amount,0)',$true); Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Big' 'Sum Big' -4157 }
        Case 'calc-field-arith' $SRC { param($p) [void]$p.CalculatedFields().Add('Mix','=Amount*1.1-Units',$true); Row $p 'Product'; Add-Data $p 'Mix' 'Sum Mix' -4157; Add-Data $p 'Mix' 'Avg Mix' -4106 }
        Case 'calc-field-of-calc' $SRC { param($p) [void]$p.CalculatedFields().Add('Half','=Amount/2',$true); [void]$p.CalculatedFields().Add('HalfPlus','=Half+Units',$true); Row $p 'Region'; Add-Data $p 'HalfPlus' 'Sum HalfPlus' -4157 }
        Case 'calc-item-sum' $SRC { param($p) Row $p 'Product'; Col $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; [void]$p.PivotFields('Product').CalculatedItems().Add('AlphaBeta','=Alpha+Beta',$true) }
        Case 'calc-item-diff' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; [void]$p.PivotFields('Region').CalculatedItems().Add('EastMinusWest','=East-West',$true) }
        Case 'calc-item-avg-refused' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Average Amount' -4106; [void]$p.PivotFields('Region').CalculatedItems().Add('NorthSouth','=(North+South)/2',$true) }
        Case 'calc-item-half-sum' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; [void]$p.PivotFields('Region').CalculatedItems().Add('NorthSouthHalf','=(North+South)/2',$true) }
        $grp=@(@('group-month',@($false,$false,$false,$false,$true,$false,$false)),@('group-year',@($false,$false,$false,$false,$false,$false,$true)),@('group-month-year',@($false,$false,$false,$false,$true,$false,$true)),@('group-quarter-year',@($false,$false,$false,$false,$false,$true,$true)),@('group-quarter',@($false,$false,$false,$false,$false,$true,$false)),@('group-day-month',@($false,$false,$false,$true,$true,$false,$false)))
        foreach($g in $grp){ $gg=$g; Case $gg[0] $SRC { param($p) Row $p 'Month'; Add-Data $p 'Amount' 'Sum Amount' -4157; [void](Call $p.PivotFields('Month').DataRange.Cells.Item(1,1) 'Group' @($true,$true,[Type]::Missing,[object[]]$gg[1]))  } }
        Case 'group-days-7' $SRC { param($p) Row $p 'Month'; Add-Data $p 'Amount' 'Sum Amount' -4157; [void](Call $p.PivotFields('Month').DataRange.Cells.Item(1,1) 'Group' @(45306.0,46000.0,7,[object[]]@($false,$false,$false,$true,$false,$false,$false))) }
        Case 'group-year-columns' $SRC { param($p) Row $p 'Region'; Col $p 'Month'; Add-Data $p 'Amount' 'Sum Amount' -4157; [void](Call $p.PivotFields('Month').DataRange.Cells.Item(1,1) 'Group' @($true,$true,[Type]::Missing,[object[]]@($false,$false,$false,$false,$false,$true,$true))) }
        Case 'group-number-amount' $SRC { param($p) Row $p 'Amount'; Add-Data $p 'Units' 'Sum Units' -4157; [void](Call $p.PivotFields('Amount').DataRange.Cells.Item(1,1) 'Group' @(0.0,400.0,100.0)) }
        Case 'group-number-units' $SRC { param($p) Row $p 'Units'; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Amount' 'Count Amount' -4112; [void](Call $p.PivotFields('Units').DataRange.Cells.Item(1,1) 'Group' @(1.0,5.0,2.0)) }
        Case 'group-text-items' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; $r=$p.PivotFields('Region').DataRange; [void](Call $r.Cells.Item(1,1).Resize(2,1) 'Group') }
        Case 'values-rows-region' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $p.DataPivotField.Orientation=1 }
        Case 'values-rows-two-fields' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $p.DataPivotField.Orientation=1; $p.RowAxisLayout(1) }
        Case 'values-rows-first' $SRC { param($p) Row $p 'Region' 1; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $p.DataPivotField.Orientation=1; $p.DataPivotField.Position=1 }
        Case 'values-rows-with-columns' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $p.DataPivotField.Orientation=1 }
        Case 'values-columns-after-field' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $p.DataPivotField.Position=1 }
        $sub=@($false,$true,$true,$true,$true,$true,$false,$false,$false,$false,$false,$false)
        $suball=@($false,$true,$true,$true,$true,$true,$true,$true,$true,$true,$true,$true)
        $subnone=@($false,$false,$false,$false,$false,$false,$false,$false,$false,$false,$false,$false)
        Case 'subtotals-multiple-compact' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; Set-Member $p.PivotFields('Region') 'Subtotals' ([object[]]$sub) }
        Case 'subtotals-multiple-outline-top' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.RowAxisLayout(2); $p.SubtotalLocation(1); Set-Member $p.PivotFields('Region') 'Subtotals' ([object[]]$sub) }
        Case 'subtotals-multiple-outline-bottom' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.RowAxisLayout(2); $p.SubtotalLocation(2); Set-Member $p.PivotFields('Region') 'Subtotals' ([object[]]$sub) }
        Case 'subtotals-multiple-tabular' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.RowAxisLayout(1); Set-Member $p.PivotFields('Region') 'Subtotals' ([object[]]$sub) }
        Case 'subtotals-all-eleven' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.RowAxisLayout(2); Set-Member $p.PivotFields('Region') 'Subtotals' ([object[]]$suball) }
        Case 'subtotals-multiple-two-data' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $p.RowAxisLayout(2); Set-Member $p.PivotFields('Region') 'Subtotals' ([object[]]$sub) }
        Case 'subtotals-none' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.RowAxisLayout(2); Set-Member $p.PivotFields('Region') 'Subtotals' ([object[]]$subnone) }
        Case 'blank-line-outer' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.RowAxisLayout(1); $p.PivotFields('Region').LayoutBlankLine=$true }
        Case 'blank-line-both' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.RowAxisLayout(2); $p.PivotFields('Region').LayoutBlankLine=$true; $p.PivotFields('Product').LayoutBlankLine=$true }
        Case 'blank-line-compact-columns' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Col $p 'Month'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.PivotFields('Region').LayoutBlankLine=$true }
        Case 'nodata-rows' $SRC { param($p) $m=$p.PivotFields('Month'); $m.Orientation=3; $m.CurrentPage=[string]$m.PivotItems(1).Name; Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.PivotFields('Region').ShowAllItems=$true; $p.PivotFields('Product').ShowAllItems=$true }
        Case 'nodata-rows-hidden' $SRC { param($p) $m=$p.PivotFields('Month'); $m.Orientation=3; $m.CurrentPage=[string]$m.PivotItems(1).Name; Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157 }
        Case 'nodata-nullstring' $SRC { param($p) $m=$p.PivotFields('Month'); $m.Orientation=3; $m.CurrentPage=[string]$m.PivotItems(1).Name; Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.PivotFields('Region').ShowAllItems=$true; $p.PivotFields('Product').ShowAllItems=$true; $p.NullString='-'; $p.DisplayNullString=$true }
        Case 'nodata-two-row-fields' $SRC { param($p) $m=$p.PivotFields('Month'); $m.Orientation=3; $m.CurrentPage=[string]$m.PivotItems(1).Name; Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.PivotFields('Region').ShowAllItems=$true; $p.PivotFields('Product').ShowAllItems=$true; $p.RowAxisLayout(1) }
        Case 'sort-by-value-desc' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.PivotFields('Region').AutoSort(2,'Sum Amount') }
        Case 'sort-by-value-asc' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.PivotFields('Region').AutoSort(1,'Sum Amount') }
        Case 'sort-by-second-data' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $p.PivotFields('Region').AutoSort(2,'Sum Units') }
        Case 'sort-by-value-inner' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.PivotFields('Product').AutoSort(2,'Sum Amount'); $p.RowAxisLayout(1) }
        Case 'sort-by-value-columns' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.PivotFields('Product').AutoSort(2,'Sum Amount') }
        Case 'sort-by-value-ties' $SRC { param($p) Row $p 'Product'; Add-Data $p 'Units' 'Sum Units' -4157; $p.PivotFields('Product').AutoSort(2,'Sum Units') }
        $ERC='Errs!R1C1:R'+$script:errsRows+'C3'
        Case 'err-values' $ERC { param($p) Row $p 'Cat'; foreach($f in $fns[0..5]){Add-Data $p 'Val' ($f[0]+' Val') $f[1]} }
        Case 'err-values-display' $ERC { param($p) Row $p 'Cat'; Add-Data $p 'Val' 'Sum Val' -4157; Add-Data $p 'Val' 'Count Val' -4112; $p.ErrorString='ERR'; $p.DisplayErrorString=$true }
        Case 'err-keys' $ERC { param($p) Row $p 'Key'; Add-Data $p 'Val' 'Count Val' -4112; Add-Data $p 'Val' 'CountNums Val' -4113 }
        Case 'err-keys-desc' $ERC { param($p) Row $p 'Key'; Add-Data $p 'Val' 'Count Val' -4112; $p.PivotFields('Key').AutoSort(2,'Key') }
        Case 'err-keys-columns' $ERC { param($p) Row $p 'Cat'; Col $p 'Key'; Add-Data $p 'Val' 'Count Val' -4112 }
        $PRC='Prec!R1C1:R'+$script:precRows+'C3'
        Case 'prec-all-functions' $PRC { param($p) Row $p 'Grp'; foreach($f in $fns){Add-Data $p 'Val' ($f[0]+' Val') $f[1]} }
        Case 'group-number-clean' $PRC { param($p) Row $p 'Ord'; Add-Data $p 'Val' 'Sum Val' -4157; Add-Data $p 'Val' 'Count Val' -4112; [void](Call $p.PivotFields('Ord').DataRange.Cells.Item(1,1) 'Group' @(1.0,10.0,3.0)) }
        Case 'prec-sum-average' $PRC { param($p) Row $p 'Grp'; Add-Data $p 'Val' 'Sum Val' -4157; Add-Data $p 'Val' 'Average Val' -4106; Add-Data $p 'Ord' 'Sum Ord' -4157 }
        Case 'prec-grand-only' $PRC { param($p) Add-Data $p 'Val' 'Sum Val' -4157; Add-Data $p 'Val' 'Product Val' -4149 }
        Case 'prec-column-grand' $PRC { param($p) Row $p 'Grp'; Col $p 'Ord'; Add-Data $p 'Val' 'Sum Val' -4157 }
        # Missing items: remove a category, refresh with the default retention, then with none, then add it back.
        [void](Try-Step 'missing-items' {
            $ws=$book.Worksheets.Add([Type]::Missing,$book.Worksheets.Item($book.Worksheets.Count));$ws.Name='MissingItems'
            $cache=$book.PivotCaches().Create(1,'SalesTable',8)
            $p=$cache.CreatePivotTable($ws.Range('A3'),'pvt_missing_items',$true,8)
            Row $p 'Product'; Col $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157
            Stage-Pivot 'missing-items-built' $p
            $src=$book.Worksheets.Item('Source')
            foreach($c in $src.Range('B2:B37').Cells){if($c.Value2 -eq 'Gamma'){$c.Value2='Alpha'}}
            [void]$p.RefreshTable();Stage-Pivot 'missing-items-refresh-default' $p
            $p.PivotCache().MissingItemsLimit=0;[void]$p.RefreshTable();Stage-Pivot 'missing-items-refresh-none' $p
            $src.Range('B5').Value2='Gamma';$src.Range('B9').Value2='Delta'
            [void]$p.RefreshTable();Stage-Pivot 'missing-items-refresh-added' $p
            $p.PivotCache().MissingItemsLimit=-1;$src.Range('B9').Value2='Alpha';[void]$p.RefreshTable();Stage-Pivot 'missing-items-delta-removed-auto' $p
            $p.PivotCache().MissingItemsLimit=32500;[void]$p.RefreshTable();Stage-Pivot 'missing-items-max-limit' $p
        })
        Save 'm1.xlsx'
     }
     'm2' {
        # M2 formatting: what Excel displays for pivot output cells (DisplayFormat), for table styles with their
        # options, data-field number formats, formatted areas, preserve-formatting and column autofit on refresh.
        $excel.Calculation=-4105
        $script:fmtCells=$true
        [void](Build-Source $book)
        $SRC='Source!R1C1:R37C5'
        $styles='PivotStyleLight16','PivotStyleLight1','PivotStyleLight8','PivotStyleLight12','PivotStyleLight21','PivotStyleMedium1','PivotStyleMedium2','PivotStyleMedium9','PivotStyleMedium14','PivotStyleMedium28','PivotStyleDark1','PivotStyleDark6','PivotStyleDark11'
        foreach($name in $styles){
            $style=$name
            Case ('style-rows-'+$style) $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $p.TableStyle2=$style }
            Case ('style-cols-'+$style) $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.TableStyle2=$style }
        }
        Case 'style-none-rows' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $p.TableStyle2='' }
        Case 'style-none-cols' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.TableStyle2='' }
        $flags=@(@('no-row-headers','ShowTableStyleRowHeaders',$false),@('no-column-headers','ShowTableStyleColumnHeaders',$false),@('row-stripes','ShowTableStyleRowStripes',$true),@('column-stripes','ShowTableStyleColumnStripes',$true),@('last-column','ShowTableStyleLastColumn',$true))
        foreach($flag in $flags){
            $fl=$flag
            Case ('option-medium9-'+$fl[0]) $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.TableStyle2='PivotStyleMedium9'; Set-Member $p $fl[1] $fl[2] }
            Case ('option-light16-'+$fl[0]) $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; Set-Member $p $fl[1] $fl[2] }
        }
        Case 'option-medium9-all-stripes' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Col $p 'Month'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.TableStyle2='PivotStyleMedium9'; $p.ShowTableStyleRowStripes=$true; $p.ShowTableStyleColumnStripes=$true }
        Case 'layout-compact-medium2' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.TableStyle2='PivotStyleMedium2'; $p.RowAxisLayout(0) }
        Case 'layout-outline-medium2' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.TableStyle2='PivotStyleMedium2'; $p.RowAxisLayout(2) }
        Case 'layout-tabular-medium2' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.TableStyle2='PivotStyleMedium2'; $p.RowAxisLayout(1) }
        Case 'layout-page-filter-medium2' $SRC { param($p) $x=$p.PivotFields('Product'); $x.Orientation=3; Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.TableStyle2='PivotStyleMedium2' }
        Case 'layout-blank-line-medium2' $SRC { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.TableStyle2='PivotStyleMedium2'; $p.RowAxisLayout(1); $p.PivotFields('Region').LayoutBlankLine=$true }
        $numfmts=@(@('number-2dp','#,##0.00','0'),@('percent','0.0%','0.00'),@('currency','$#,##0','[$EUR] #,##0.0'),@('scientific','0.00E+00','0.000'),@('text-prefix','"Amt "#,##0','"U "0'),@('general','General','@'),@('date-like','yyyy-mm-dd','h:mm'),@('negative-red','#,##0;[Red]-#,##0','#,##0;(#,##0)'))
        foreach($nf in $numfmts){
            $f=$nf
            Case ('numfmt-'+$f[0]) $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $d=Items (Call $p 'DataFields'); $d[0].NumberFormat=$f[1]; $d[1].NumberFormat=$f[2] }
        }
        Case 'numfmt-show-as-percent' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; Set-Calc $p 1 8 $null $null }
        Case 'numfmt-show-as-percent-custom' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; Set-Calc $p 1 8 $null $null; (Items (Call $p 'DataFields'))[0].NumberFormat='0.0000%' }
        Case 'numfmt-count' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Count Amount' -4112; Add-Data $p 'Amount' 'Avg Amount' -4106 }
        Case 'numfmt-source-formatted' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Month' 'Max Month' -4136 }
        Case 'area-data-field-range' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $d=Items (Call $p 'DataFields'); $d[0].DataRange.Interior.Color=65535; $d[0].DataRange.Font.Bold=$true; $d[1].DataRange.Font.Color=255 }
        Case 'area-label-range' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.PivotFields('Region').LabelRange.Interior.Color=13434879; $p.PivotFields('Region').LabelRange.Font.Italic=$true }
        Case 'area-item-label' $SRC { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.PivotFields('Region').PivotItems('East').LabelRange.Interior.Color=65280; $p.PivotFields('Region').PivotItems('East').LabelRange.Font.Bold=$true }
        Case 'area-data-cell' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.DataBodyRange.Cells.Item(2,2).Interior.Color=16711680; $p.DataBodyRange.Cells.Item(2,2).Font.Color=16777215 }
        Case 'area-data-body' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.DataBodyRange.Font.Name='Courier New'; $p.DataBodyRange.Borders.Item(9).LineStyle=1 }
        Case 'area-column-range' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.ColumnRange.Interior.Color=14277081; $p.RowRange.Font.Size=14 }
        Case 'area-grand-total-name' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.GrandTotalName='Everything' }
        Case 'area-header-names' $SRC { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157; $p.CompactLayoutRowHeader='Areas'; $p.CompactLayoutColumnHeader='Kinds' }
        # Preserve formatting and autofit across a refresh (a long label arrives; the user sets a width).
        $refreshCase={
            param($Id,[bool]$Preserve,[bool]$AutoFit,[bool]$UserWidth)
            [void](Try-Step "case-$Id" {
                $script:caseNo++
                $ws=$book.Worksheets.Add([Type]::Missing,$book.Worksheets.Item($book.Worksheets.Count));$ws.Name='C'+$script:caseNo
                $cache=$book.PivotCaches().Create(1,'SalesTable',8)
                $p=$cache.CreatePivotTable($ws.Range('A3'),('pvt_'+$Id.Replace('-','_')),$true,8)
                Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157
                $p.PreserveFormatting=$Preserve;$p.HasAutoFormat=$AutoFit
                $p.DataBodyRange.Cells.Item(1,1).Interior.Color=65535
                if($UserWidth){$ws.Columns.Item(2).ColumnWidth=30}
                Stage-Pivot ($Id+'-built') $p
                $src=$book.Worksheets.Item('Source');$old=$src.Range('B3').Value2
                $src.Range('B3').Value2='A very long product name that needs a wide column';$src.Range('D3').Value2=[double]123456789.125
                [void]$p.RefreshTable();Stage-Pivot ($Id+'-refreshed') $p
                $src.Range('B3').Value2=$old;$src.Range('D3').Value2=[double]107.25;[void]$p.RefreshTable()
            })
        }
        & $refreshCase 'refresh-preserve-autofit' $true $true $false
        & $refreshCase 'refresh-preserve-noautofit' $true $false $false
        & $refreshCase 'refresh-nopreserve-autofit' $false $true $false
        & $refreshCase 'refresh-nopreserve-noautofit' $false $false $false
        & $refreshCase 'refresh-userwidth-autofit' $true $true $true
        & $refreshCase 'refresh-userwidth-noautofit' $true $false $true
        Save 'm2.xlsx'
     }
     'm3' {
        # M3 GETPIVOTDATA oracle workbook: pivots of several shapes on their own sheets, and a GPD sheet whose
        # formulas are entered through Excel. The saved package is the oracle workbook; its values are captured
        # separately by `cargo xtask capture`. A formula Excel refuses to accept is recorded as an observation.
        $excel.Calculation=-4105
        [void](Build-Source $book)
        $SRC='Source!R1C1:R37C5'
        $specs=[ordered]@{}
        $mk={
            param($Sheet,$Name,[scriptblock]$Build)
            [void](Try-Step "pivot-$Sheet" {
                $ws=$book.Worksheets.Add([Type]::Missing,$book.Worksheets.Item($book.Worksheets.Count));$ws.Name=$Sheet
                $cache=$book.PivotCaches().Create(1,$SRC,8)
                $p=$cache.CreatePivotTable($ws.Range('A3'),$Name,$true,8)
                & $Build $p
                Stage-Pivot ('pivot-'+$Sheet) $p
            })
        }
        & $mk 'PvOne' 'pvt_one' { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum of Amount' -4157 }
        & $mk 'PvTwoData' 'pvt_two_data' { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum of Amount' -4157; Add-Data $p 'Units' 'Total Units' -4157 }
        & $mk 'PvRowCol' 'pvt_row_col' { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum of Amount' -4157 }
        & $mk 'PvTwoRow' 'pvt_two_row' { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum of Amount' -4157; $p.RowAxisLayout(1) }
        & $mk 'PvTwoRowCompact' 'pvt_two_row_compact' { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum of Amount' -4157 }
        & $mk 'PvDates' 'pvt_dates' { param($p) Row $p 'Month'; Add-Data $p 'Amount' 'Sum of Amount' -4157 }
        & $mk 'PvDatesGrouped' 'pvt_dates_grouped' { param($p) Row $p 'Month'; Add-Data $p 'Amount' 'Sum of Amount' -4157; [void](Call $p.PivotFields('Month').DataRange.Cells.Item(1,1) 'Group' @($true,$true,[Type]::Missing,[object[]]@($false,$false,$false,$false,$false,$true,$true))) }
        & $mk 'PvNumbers' 'pvt_numbers' { param($p) Row $p 'Units'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum of Amount' -4157 }
        & $mk 'PvFilter' 'pvt_filter' { param($p) $x=$p.PivotFields('Product'); $x.Orientation=3; $x.CurrentPage='Alpha'; Row $p 'Region'; Add-Data $p 'Amount' 'Sum of Amount' -4157 }
        & $mk 'PvHidden' 'pvt_hidden' { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum of Amount' -4157; $p.PivotFields('Region').PivotItems('East').Visible=$false }
        & $mk 'PvNoGrand' 'pvt_no_grand' { param($p) Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum of Amount' -4157; $p.ColumnGrand=$false; $p.RowGrand=$false }
        & $mk 'PvBlankItem' 'pvt_blank_item' { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Sum of Amount' -4157; Add-Data $p 'Amount' 'Count of Amount' -4112 }
        & $mk 'PvAverage' 'pvt_average' { param($p) Row $p 'Region'; Add-Data $p 'Amount' 'Average of Amount' -4106 }
        & $mk 'PvSubtotals' 'pvt_subtotals' { param($p) Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum of Amount' -4157; $p.RowAxisLayout(2); $p.SubtotalLocation(1) }
        # Two pivots on one sheet, side by side, to test a reference that could belong to either.
        [void](Try-Step 'pivot-PvPair' {
            $ws=$book.Worksheets.Add([Type]::Missing,$book.Worksheets.Item($book.Worksheets.Count));$ws.Name='PvPair'
            $c1=$book.PivotCaches().Create(1,$SRC,8);$p1=$c1.CreatePivotTable($ws.Range('A3'),'pvt_pair_left',$true,8);Row $p1 'Region';Add-Data $p1 'Amount' 'Sum of Amount' -4157
            $c2=$book.PivotCaches().Create(1,$SRC,8);$p2=$c2.CreatePivotTable($ws.Range('D3'),'pvt_pair_right',$true,8);Row $p2 'Product';Add-Data $p2 'Amount' 'Sum of Amount' -4157
            Stage-Pivot 'pivot-PvPair-left' $p1;Stage-Pivot 'pivot-PvPair-right' $p2
        })
        $g=$book.Worksheets.Add([Type]::Missing,$book.Worksheets.Item($book.Worksheets.Count));$g.Name='GPD'
        $g.Range('A1').Value2='case';$g.Range('B1').Value2='formula'
        $cases=[Collections.Generic.List[object]]::new()
        function Gpd([string]$Id,[string]$Formula){$cases.Add(@($Id,$Formula))}
        # Names of the data field: caption, source name, other captions, case, trailing space.
        Gpd 'name-caption' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3)'
        Gpd 'name-source' '=GETPIVOTDATA("Amount",PvOne!$A$3)'
        Gpd 'name-lower-caption' '=GETPIVOTDATA("sum of amount",PvOne!$A$3)'
        Gpd 'name-upper-source' '=GETPIVOTDATA("AMOUNT",PvOne!$A$3)'
        Gpd 'name-trailing-space' '=GETPIVOTDATA("Amount ",PvOne!$A$3)'
        Gpd 'name-unknown' '=GETPIVOTDATA("Nope",PvOne!$A$3)'
        Gpd 'name-other-pivot-caption' '=GETPIVOTDATA("Total Units",PvOne!$A$3)'
        Gpd 'name-two-data-first' '=GETPIVOTDATA("Sum of Amount",PvTwoData!$A$3)'
        Gpd 'name-two-data-second' '=GETPIVOTDATA("Total Units",PvTwoData!$A$3)'
        Gpd 'name-two-data-source-units' '=GETPIVOTDATA("Units",PvTwoData!$A$3)'
        Gpd 'name-source-vs-caption-count' '=GETPIVOTDATA("Count of Amount",PvBlankItem!$A$3)'
        Gpd 'name-source-ambiguous' '=GETPIVOTDATA("Amount",PvBlankItem!$A$3)'
        Gpd 'name-average-caption' '=GETPIVOTDATA("Average of Amount",PvAverage!$A$3)'
        Gpd 'name-from-cell' '=GETPIVOTDATA(PvOne!$B$3,PvOne!$A$3)'
        Gpd 'name-number-arg' '=GETPIVOTDATA(1,PvOne!$A$3)'
        Gpd 'name-bool-arg' '=GETPIVOTDATA(TRUE,PvOne!$A$3)'
        Gpd 'name-error-arg' '=GETPIVOTDATA(NA(),PvOne!$A$3)'
        Gpd 'name-empty-string' '=GETPIVOTDATA("",PvOne!$A$3)'
        # Totals and items.
        Gpd 'grand-one' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3)'
        Gpd 'item-east' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region","East")'
        Gpd 'item-east-lower' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"region","east")'
        Gpd 'item-east-trailing' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region","East ")'
        Gpd 'item-unknown' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region","Nowhere")'
        Gpd 'item-field-unknown' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Nofield","East")'
        Gpd 'item-field-not-in-layout' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Product","Alpha")'
        Gpd 'item-blank-label' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region","(blank)")'
        Gpd 'item-empty-string' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region","")'
        Gpd 'item-number-for-text' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region",1)'
        Gpd 'item-wildcard' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region","E*")'
        Gpd 'item-from-cell' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,PvOne!$A$3,PvOne!$A$4)'
        Gpd 'item-field-from-cell' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region",PvOne!$A$4)'
        Gpd 'item-error-arg' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region",NA())'
        Gpd 'item-field-error-arg' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,NA(),"East")'
        Gpd 'item-field-number' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,1,"East")'
        Gpd 'item-field-blank-ref' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,PvOne!$Z$40,"East")'
        Gpd 'two-data-item-first' '=GETPIVOTDATA("Sum of Amount",PvTwoData!$A$3,"Region","North")'
        Gpd 'two-data-item-second' '=GETPIVOTDATA("Total Units",PvTwoData!$A$3,"Region","North")'
        Gpd 'two-data-item-second-source' '=GETPIVOTDATA("Units",PvTwoData!$A$3,"Region","North")'
        Gpd 'rowcol-grand' '=GETPIVOTDATA("Sum of Amount",PvRowCol!$A$3)'
        Gpd 'rowcol-row-total' '=GETPIVOTDATA("Sum of Amount",PvRowCol!$A$3,"Region","East")'
        Gpd 'rowcol-column-total' '=GETPIVOTDATA("Sum of Amount",PvRowCol!$A$3,"Product","Alpha")'
        Gpd 'rowcol-cell' '=GETPIVOTDATA("Sum of Amount",PvRowCol!$A$3,"Region","East","Product","Alpha")'
        Gpd 'rowcol-cell-swapped' '=GETPIVOTDATA("Sum of Amount",PvRowCol!$A$3,"Product","Alpha","Region","East")'
        Gpd 'rowcol-cell-missing-combo' '=GETPIVOTDATA("Sum of Amount",PvRowCol!$A$3,"Region","(blank)","Product","Alpha")'
        Gpd 'rowcol-duplicate-field' '=GETPIVOTDATA("Sum of Amount",PvRowCol!$A$3,"Region","East","Region","North")'
        Gpd 'rowcol-duplicate-same-item' '=GETPIVOTDATA("Sum of Amount",PvRowCol!$A$3,"Region","East","Region","East")'
        Gpd 'rowcol-cell-from-inside' '=GETPIVOTDATA("Sum of Amount",PvRowCol!$C$6,"Region","East","Product","Alpha")'
        Gpd 'twoRow-outer' '=GETPIVOTDATA("Sum of Amount",PvTwoRow!$A$3,"Region","East")'
        Gpd 'twoRow-inner-only' '=GETPIVOTDATA("Sum of Amount",PvTwoRow!$A$3,"Product","Alpha")'
        Gpd 'twoRow-both' '=GETPIVOTDATA("Sum of Amount",PvTwoRow!$A$3,"Region","East","Product","Alpha")'
        Gpd 'twoRow-inner-then-outer' '=GETPIVOTDATA("Sum of Amount",PvTwoRow!$A$3,"Product","Alpha","Region","East")'
        Gpd 'twoRowCompact-outer' '=GETPIVOTDATA("Sum of Amount",PvTwoRowCompact!$A$3,"Region","East")'
        Gpd 'twoRowCompact-both' '=GETPIVOTDATA("Sum of Amount",PvTwoRowCompact!$A$3,"Region","East","Product","Beta")'
        Gpd 'twoRowCompact-inner-only' '=GETPIVOTDATA("Sum of Amount",PvTwoRowCompact!$A$3,"Product","Beta")'
        Gpd 'subtotal-outer' '=GETPIVOTDATA("Sum of Amount",PvSubtotals!$A$3,"Region","South")'
        Gpd 'subtotal-both' '=GETPIVOTDATA("Sum of Amount",PvSubtotals!$A$3,"Region","South","Product","Gamma")'
        Gpd 'subtotal-grand' '=GETPIVOTDATA("Sum of Amount",PvSubtotals!$A$3)'
        Gpd 'nogrand-whole' '=GETPIVOTDATA("Sum of Amount",PvNoGrand!$A$3)'
        Gpd 'nogrand-row' '=GETPIVOTDATA("Sum of Amount",PvNoGrand!$A$3,"Region","East")'
        Gpd 'nogrand-cell' '=GETPIVOTDATA("Sum of Amount",PvNoGrand!$A$3,"Region","East","Product","Alpha")'
        Gpd 'nogrand-column' '=GETPIVOTDATA("Sum of Amount",PvNoGrand!$A$3,"Product","Alpha")'
        # Dates, numbers and grouped labels.
        Gpd 'date-serial' '=GETPIVOTDATA("Sum of Amount",PvDates!$A$3,"Month",45306)'
        Gpd 'date-function' '=GETPIVOTDATA("Sum of Amount",PvDates!$A$3,"Month",DATE(2024,1,15))'
        Gpd 'date-text-us' '=GETPIVOTDATA("Sum of Amount",PvDates!$A$3,"Month","1/15/2024")'
        Gpd 'date-text-iso' '=GETPIVOTDATA("Sum of Amount",PvDates!$A$3,"Month","2024-01-15")'
        Gpd 'date-text-long' '=GETPIVOTDATA("Sum of Amount",PvDates!$A$3,"Month","15-Jan-2024")'
        Gpd 'date-text-datevalue' '=GETPIVOTDATA("Sum of Amount",PvDates!$A$3,"Month",DATEVALUE("2024-01-15"))'
        Gpd 'date-not-in-pivot' '=GETPIVOTDATA("Sum of Amount",PvDates!$A$3,"Month",DATE(2025,6,1))'
        Gpd 'date-with-time' '=GETPIVOTDATA("Sum of Amount",PvDates!$A$3,"Month",45306.5)'
        Gpd 'date-from-cell' '=GETPIVOTDATA("Sum of Amount",PvDates!$A$3,"Month",PvDates!$A$4)'
        Gpd 'grouped-year-number' '=GETPIVOTDATA("Sum of Amount",PvDatesGrouped!$A$3,"Month",2024)'
        Gpd 'grouped-year-text' '=GETPIVOTDATA("Sum of Amount",PvDatesGrouped!$A$3,"Month","2024")'
        Gpd 'grouped-quarter' '=GETPIVOTDATA("Sum of Amount",PvDatesGrouped!$A$3,"Month","Qtr1")'
        Gpd 'grouped-quarter-lower' '=GETPIVOTDATA("Sum of Amount",PvDatesGrouped!$A$3,"Month","qtr1")'
        Gpd 'grouped-quarter-and-year' '=GETPIVOTDATA("Sum of Amount",PvDatesGrouped!$A$3,"Month","Qtr1","Years",2024)'
        Gpd 'grouped-years-field' '=GETPIVOTDATA("Sum of Amount",PvDatesGrouped!$A$3,"Years",2024)'
        Gpd 'grouped-quarters-field' '=GETPIVOTDATA("Sum of Amount",PvDatesGrouped!$A$3,"Quarters","Qtr2")'
        Gpd 'grouped-date-underlying' '=GETPIVOTDATA("Sum of Amount",PvDatesGrouped!$A$3,"Month",DATE(2024,1,15))'
        Gpd 'number-item' '=GETPIVOTDATA("Sum of Amount",PvNumbers!$A$3,"Units",1)'
        Gpd 'number-item-text' '=GETPIVOTDATA("Sum of Amount",PvNumbers!$A$3,"Units","1")'
        Gpd 'number-item-float' '=GETPIVOTDATA("Sum of Amount",PvNumbers!$A$3,"Units",1.0000000000001)'
        Gpd 'number-item-text-float' '=GETPIVOTDATA("Sum of Amount",PvNumbers!$A$3,"Units","1.0")'
        Gpd 'number-item-negative' '=GETPIVOTDATA("Sum of Amount",PvNumbers!$A$3,"Units",-1)'
        Gpd 'number-item-bool' '=GETPIVOTDATA("Sum of Amount",PvNumbers!$A$3,"Units",TRUE)'
        Gpd 'number-item-column' '=GETPIVOTDATA("Sum of Amount",PvNumbers!$A$3,"Units",2,"Product","Beta")'
        # Filters and hidden items.
        Gpd 'filter-grand' '=GETPIVOTDATA("Sum of Amount",PvFilter!$A$5)'
        Gpd 'filter-item' '=GETPIVOTDATA("Sum of Amount",PvFilter!$A$5,"Region","East")'
        Gpd 'filter-page-field-match' '=GETPIVOTDATA("Sum of Amount",PvFilter!$A$5,"Product","Alpha")'
        Gpd 'filter-page-field-other' '=GETPIVOTDATA("Sum of Amount",PvFilter!$A$5,"Product","Beta")'
        Gpd 'filter-page-and-row' '=GETPIVOTDATA("Sum of Amount",PvFilter!$A$5,"Product","Alpha","Region","East")'
        Gpd 'filter-ref-at-page-cell' '=GETPIVOTDATA("Sum of Amount",PvFilter!$A$3,"Region","East")'
        Gpd 'hidden-item' '=GETPIVOTDATA("Sum of Amount",PvHidden!$A$3,"Region","East")'
        Gpd 'hidden-other-item' '=GETPIVOTDATA("Sum of Amount",PvHidden!$A$3,"Region","North")'
        Gpd 'hidden-grand' '=GETPIVOTDATA("Sum of Amount",PvHidden!$A$3)'
        # References: outside, empty, other sheet, a range, several cells, pairs of pivots.
        Gpd 'ref-outside-same-sheet' '=GETPIVOTDATA("Sum of Amount",PvOne!$Z$50)'
        Gpd 'ref-source-sheet' '=GETPIVOTDATA("Sum of Amount",Source!$A$1)'
        Gpd 'ref-column-range' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3:$B$9)'
        Gpd 'ref-whole-column' '=GETPIVOTDATA("Sum of Amount",PvOne!$A:$A)'
        Gpd 'ref-inside-data' '=GETPIVOTDATA("Sum of Amount",PvOne!$B$5)'
        Gpd 'ref-inside-label' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$5)'
        Gpd 'ref-header-row' '=GETPIVOTDATA("Sum of Amount",PvOne!$B$3)'
        Gpd 'ref-text-address' '=GETPIVOTDATA("Sum of Amount","PvOne!$A$3")'
        Gpd 'ref-text-range-name' '=GETPIVOTDATA("Sum of Amount","PvOne")'
        Gpd 'ref-pair-left' '=GETPIVOTDATA("Sum of Amount",PvPair!$A$3,"Region","East")'
        Gpd 'ref-pair-right' '=GETPIVOTDATA("Sum of Amount",PvPair!$D$3,"Product","Alpha")'
        Gpd 'ref-pair-right-with-left-field' '=GETPIVOTDATA("Sum of Amount",PvPair!$D$3,"Region","East")'
        Gpd 'ref-pair-middle' '=GETPIVOTDATA("Sum of Amount",PvPair!$C$3,"Region","East")'
        Gpd 'ref-pair-range-both' '=GETPIVOTDATA("Sum of Amount",PvPair!$A$3:$E$9,"Region","East")'
        Gpd 'ref-error' '=GETPIVOTDATA("Sum of Amount",NA())'
        Gpd 'ref-number' '=GETPIVOTDATA("Sum of Amount",5)'
        Gpd 'ref-indirect' '=GETPIVOTDATA("Sum of Amount",INDIRECT("PvOne!A3"),"Region","East")'
        Gpd 'ref-offset' '=GETPIVOTDATA("Sum of Amount",OFFSET(PvOne!$A$3,2,0),"Region","East")'
        # Values as the result, used further.
        Gpd 'result-in-arithmetic' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region","East")*2'
        Gpd 'result-in-sum' '=SUM(GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region","East"),1)'
        Gpd 'result-text-field' '=GETPIVOTDATA("Count of Amount",PvBlankItem!$A$3,"Region","East")'
        Gpd 'result-average' '=GETPIVOTDATA("Average of Amount",PvAverage!$A$3,"Region","South")'
        Gpd 'result-nested' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region",GETPIVOTDATA("Region",PvOne!$A$3))'
        # MDX form (valid for OLAP and data model pivots; recorded against ordinary pivots).
        Gpd 'mdx-measure' '=GETPIVOTDATA("[Measures].[Sum of Amount]",PvOne!$A$3)'
        Gpd 'mdx-measure-member' '=GETPIVOTDATA("[Measures].[Sum of Amount]",PvOne!$A$3,"[Source].[Region]","[Source].[Region].&[East]")'
        Gpd 'mdx-one-arg-mdx' '=GETPIVOTDATA(PvOne!$A$3,"[Region].[East]")'
        Gpd 'mdx-pivot-first-text' '=GETPIVOTDATA(PvOne!$A$3,"[Measures].[Sum of Amount]")'
        # Argument counts and shapes that Excel may refuse when the formula is entered.
        Gpd 'args-one' '=GETPIVOTDATA("Sum of Amount")'
        Gpd 'args-three' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region")'
        Gpd 'args-five' '=GETPIVOTDATA("Sum of Amount",PvOne!$A$3,"Region","East","Product")'
        Gpd 'args-zero' '=GETPIVOTDATA()'
        Gpd 'args-many' '=GETPIVOTDATA("Sum of Amount",PvRowCol!$A$3,"Region","East","Product","Alpha","Region","East","Product","Alpha","Region","East","Product","Alpha")'
        $row=1
        foreach($c in $cases){
            $row++
            $g.Cells.Item($row,1).Value2=$c[0]
            $id=$c[0];$formula=$c[1]
            [void](Try-Step "entry-$id" { $g.Cells.Item($row,2).Formula=$formula })
        }
        $obs.Add(@{stage='gpd-entered';cases=$cases.Count;last_row=$row})
        $excel.Calculate()
        $values=[Collections.Generic.List[object]]::new()
        for($r=2;$r -le $row;$r++){$cell=$g.Cells.Item($r,2);$values.Add([ordered]@{id=[string]$g.Cells.Item($r,1).Value2;formula=[string]$cell.Formula;has_formula=[bool]$cell.HasFormula;value=(Enc $cell.Value2);text=[string]$cell.Text})}
        $obs.Add(@{stage='gpd-values';values=$values.ToArray()})
        Save 'oracle.xlsx'
     }
     'm4' {
        # M4 CUBE oracle over an in-file data model: the table is loaded into the model, measures are added,
        # the CUBE formulas of pivot-m4-formulas.tsv are entered on Sheet1 column A and calculated with
        # ScreenUpdating on (asynchronous queries), then an edit-then-refresh sequence, a model-only table, a
        # pivot over the model with GETPIVOTDATA in its MDX form, and Convert to Formulas are recorded.
        $excel.Calculation=-4105
        $excel.ScreenUpdating=$true
        $ws=$book.Worksheets.Item(1);$ws.Name='Sheet1'
        $data=@(@('Region','Product','Amount'),@('East','A',10),@('West','A',20),@('East','B',5),@('North','B',7),@('West','C',0.1),@('East','A',0.2))
        for($r=0;$r -lt $data.Count;$r++){for($c=0;$c -lt 3;$c++){$ws.Cells.Item($r+1,20+$c).Value2=$data[$r][$c]}}
        $lo=$ws.ListObjects.Add(1,$ws.Range('T1:V7'),$null,1);$lo.Name='Table1'
        $modelPath=Join-Path $dir 'm4-model.xlsx'
        $book.SaveAs($modelPath,51)
        $model=$null
        [void](Try-Step 'model-connection' {
            [void]$book.Connections.Add2('WorksheetConnection_Table1','',('WORKSHEET;'+$modelPath),'Table1',7,$true,$false)
            $script:model=$book.Model
        })
        $model=$script:model
        $obs.Add(@{stage='model-created';connections=(Map $book.Connections {param($c)@{name=[string]$c.Name;type=[int]$c.Type}});model_tables=$(try{Map $model.ModelTables {param($t)[string]$t.Name}}catch{@{unread=$_.Exception.Message}})})
        $table=$null
        [void](Try-Step 'model-table' { $script:table=$model.ModelTables.Item('Table1') })
        $table=$script:table
        $measures=@(@('TotalAmount','SUM(Table1[Amount])'),@('CountAmount','COUNT(Table1[Amount])'),@('AvgAmount','AVERAGE(Table1[Amount])'),@('MinAmount','MIN(Table1[Amount])'),@('MaxAmount','MAX(Table1[Amount])'),@('DistinctRegion','DISTINCTCOUNT(Table1[Region])'),@('RowCount','COUNTROWS(Table1)'))
        foreach($m in $measures){
            $mm=$m
            [void](Try-Step ('measure-'+$mm[0]) { [void]$model.ModelMeasures.Add($mm[0],$table,$mm[1],$model.ModelFormatGeneral) })
        }
        $obs.Add(@{stage='measures-added';measures=$(try{Map $model.ModelMeasures {param($m)@{name=[string]$m.Name;formula=[string]$m.Formula}}}catch{@{unread=$_.Exception.Message}})})
        # CUBE formulas.
        $rows=[Collections.Generic.List[object]]::new()
        foreach($line in Get-Content -LiteralPath $(if($req.formulas){[string]$req.formulas}else{Join-Path $PSScriptRoot 'pivot-m4-formulas.tsv'}) -Encoding utf8){
            if($line.StartsWith('#') -or -not $line.Trim()){continue}
            $parts=$line.Split("`t",2);$rows.Add(@([int]$parts[0],$parts[1]))
        }
        foreach($entry in $rows){
            $rowNo=$entry[0];$formula='='+$entry[1]
            # Excel answers 0x800AC472 while an asynchronous query is running: wait for it, then enter again.
            [void](Try-Step ("entry-A$rowNo") {
                for($attempt=1;;$attempt++){
                    try{ $ws.Cells.Item($rowNo,1).Formula=$formula; break }
                    catch{ if($attempt -ge 40 -or $_.Exception.HResult -ne -2146777998){throw}; try{$excel.CalculateUntilAsyncQueriesDone()}catch{}; Start-Sleep -Milliseconds 250 }
                }
            })
        }
        function Read-Cube([string]$Stage){
            $vals=[Collections.Generic.List[object]]::new()
            foreach($entry in $rows){$cell=$ws.Cells.Item($entry[0],1);$vals.Add([ordered]@{row=$entry[0];formula=[string]$cell.Formula;value=(Enc $cell.Value2);text=[string]$cell.Text})}
            $obs.Add(@{stage=$Stage;utc=[DateTime]::UtcNow.ToString('O');cube=$vals.ToArray()})
        }
        Read-Cube 'cube-entered'
        [void](Try-Step 'calculate-full' { $excel.CalculateFull() })
        # Asynchronous queries finish while Excel is not busy: wait for them.
        for($i=0;$i -lt 60;$i++){ $state=[int]$excel.CalculationState; if($state -eq 0){break}; Start-Sleep -Milliseconds 500; try{$excel.CalculateUntilAsyncQueriesDone()}catch{} }
        Read-Cube 'cube-calculated'
        $obs.Add(@{stage='calculation-state';state=[int]$excel.CalculationState})
        Save 'm4-cube.xlsx'
        # Edit then refresh: the model keeps its snapshot until a refresh.
        [void](Try-Step 'edit-source-cell' { $ws.Range('V2').Value2=[double]100 })
        [void](Try-Step 'calculate-after-edit' { $excel.Calculate();$excel.CalculateUntilAsyncQueriesDone() })
        Read-Cube 'cube-after-edit-no-refresh'
        [void](Try-Step 'refresh-connection' { $book.Connections.Item('ThisWorkbookDataModel').Refresh();$excel.CalculateUntilAsyncQueriesDone() })
        Read-Cube 'cube-after-connection-refresh'
        [void](Try-Step 'refresh-all' { $book.RefreshAll();$excel.CalculateUntilAsyncQueriesDone();$excel.CalculateFull();$excel.CalculateUntilAsyncQueriesDone() })
        Read-Cube 'cube-after-refresh-all'
        [void](Try-Step 'add-row' { $lo.ListRows.Add()|Out-Null;$ws.Range('T8').Value2='South';$ws.Range('U8').Value2='D';$ws.Range('V8').Value2=[double]3 })
        Read-Cube 'cube-after-add-row-no-refresh'
        [void](Try-Step 'refresh-all-after-add-row' { $book.RefreshAll();$excel.CalculateUntilAsyncQueriesDone();$excel.CalculateFull();$excel.CalculateUntilAsyncQueriesDone() })
        Read-Cube 'cube-after-add-row-refresh'
        Save 'm4-cube-after-refresh.xlsx'
        # A pivot over the model, GETPIVOTDATA on it and Convert to Formulas.
        $pv=$book.Worksheets.Add([Type]::Missing,$ws);$pv.Name='ModelPivot'
        $pt=$null
        [void](Try-Step 'model-pivot' {
            $cache=$book.PivotCaches().Create(2,$book.Connections.Item('ThisWorkbookDataModel'),8)
            $script:pt=$cache.CreatePivotTable($pv.Range('A3'),'pvt_model',$true,8)
        })
        $pt=$script:pt
        [void](Try-Step 'model-pivot-fields' {
            $pt.CubeFields.Item('[Table1].[Region]').Orientation=1
            $pt.CubeFields.Item('[Table1].[Product]').Orientation=2
            $pt.CubeFields.Item('[Measures].[TotalAmount]').Orientation=4
        })
        [void](Try-Step 'model-pivot-read' {
            $obs.Add(@{stage='model-pivot';range=[string]$pt.TableRange2.Address();cells=(Range-Read $pt.TableRange2)})
        })
        $g=$book.Worksheets.Add([Type]::Missing,$pv);$g.Name='GpdModel'
        $mdx=@(
         '=GETPIVOTDATA("[Measures].[TotalAmount]",ModelPivot!$A$3)',
         '=GETPIVOTDATA("[Measures].[TotalAmount]",ModelPivot!$A$3,"[Table1].[Region]","[Table1].[Region].&[East]")',
         '=GETPIVOTDATA("[Measures].[TotalAmount]",ModelPivot!$A$3,"[Table1].[Region]","[Table1].[Region].&[East]","[Table1].[Product]","[Table1].[Product].&[A]")',
         '=GETPIVOTDATA("[Measures].[TotalAmount]",ModelPivot!$A$3,"[Table1].[Product]","[Table1].[Product].&[A]")',
         '=GETPIVOTDATA("[Measures].[TotalAmount]",ModelPivot!$A$3,"[Table1].[Region]","[Table1].[Region].&[Nowhere]")',
         '=GETPIVOTDATA("Total",ModelPivot!$A$3)',
         '=GETPIVOTDATA("TotalAmount",ModelPivot!$A$3)',
         '=GETPIVOTDATA("Total",ModelPivot!$A$3,"Region","East")',
         '=GETPIVOTDATA("[Measures].[CountAmount]",ModelPivot!$A$3)',
         '=GETPIVOTDATA("[Measures].[TotalAmount]",ModelPivot!$A$3,"[Table1].[Region]","East")',
         '=GETPIVOTDATA("[Measures].[TotalAmount]",ModelPivot!$A$3,"[Table1].[Region].[Region]","[Table1].[Region].[Region].&[East]")')
        $gv=[Collections.Generic.List[object]]::new()
        $rowNo=0
        foreach($f in $mdx){
            $rowNo++;$g.Cells.Item($rowNo,1).Value2="mdx-$rowNo"
            $ff=$f;[void](Try-Step "entry-mdx-$rowNo" { $g.Cells.Item($rowNo,2).Formula=$ff })
        }
        [void](Try-Step 'calc-mdx' { $excel.Calculate();$excel.CalculateUntilAsyncQueriesDone() })
        for($r=1;$r -le $rowNo;$r++){$cell=$g.Cells.Item($r,2);$gv.Add([ordered]@{row=$r;formula=[string]$cell.Formula;value=(Enc $cell.Value2);text=[string]$cell.Text})}
        $obs.Add(@{stage='gpd-model';values=$gv.ToArray()})
        Save 'm4-model-pivot.xlsx'
        [void](Try-Step 'convert-to-formulas' { [void]$pt.ConvertToFormulas($true) })
        [void](Try-Step 'converted-read' {
            $conv=[Collections.Generic.List[object]]::new()
            foreach($c in $pv.UsedRange.Cells){if($c.HasFormula){$conv.Add([ordered]@{address=[string]$c.Address($false,$false);formula=[string]$c.Formula;value=(Enc $c.Value2);text=[string]$c.Text})}}
            $obs.Add(@{stage='converted';used=[string]$pv.UsedRange.Address();formulas=$conv.ToArray()})
        })
        Save 'm4-converted.xlsx'
        # Model-only table: remove the worksheet table; the model keeps its copy.
        [void](Try-Step 'delete-worksheet-table' { $lo.Delete() })
        [void](Try-Step 'calc-after-table-delete' { $excel.CalculateFull();$excel.CalculateUntilAsyncQueriesDone() })
        Read-Cube 'cube-after-worksheet-table-deleted'
        [void](Try-Step 'refresh-after-table-deleted' { $book.Connections.Item('ThisWorkbookDataModel').Refresh();$excel.CalculateUntilAsyncQueriesDone() })
        Read-Cube 'cube-after-table-deleted-refresh'
        Save 'm4-model-only.xlsx'
     }
     'm5' {
        # M5 structural and layout edits (bead dc-8wp, plan package M5): rows and columns inserted or deleted around a
        # pivot output and around worksheet-range and table sources, table growth, overlap, layout changes on an
        # unrefreshed pivot. One workbook per scenario (Src + Out), alerts off (every Excel default answer is
        # retained as what happened; no dialog is answered). `scenario` selects scenario ids by -like pattern.
        $excel.Calculation=-4105
        $filter=if($req.scenario){[string]$req.scenario}else{'*'}
        function ColL([int]$n){$s='';while($n -gt 0){$s=[string][char](65+(($n-1)%26))+$s;$n=[int][math]::Floor(($n-1)/26)};return $s}
        function Dump5($ws,[string]$addr){
            $l=[Collections.Generic.List[object]]::new()
            try{
                $r=$ws.Range($addr);$v=$r.Value2;$rows=$r.Rows.Count;$cols=$r.Columns.Count;$r0=$r.Row;$c0=$r.Column
                for($i=1;$i -le $rows;$i++){for($j=1;$j -le $cols;$j++){$x=$v[$i,$j];if($null -ne $x){$l.Add(@{a=((ColL ($c0+$j-1))+($r0+$i-1));v=(Enc $x)})}}}
            }catch{$l.Add(@{unread=$_.Exception.Message})}
            return ,$l.ToArray()
        }
        function Extra5(){
            $e=[ordered]@{}
            foreach($n in 'Src','Out'){
                try{$ws=$script:book.Worksheets.Item($n);$e[$n]=@{cells=(Dump5 $ws 'A1:P40');tables=@(foreach($lo in (Items (Call $ws 'ListObjects'))){@{name=[string]$lo.Name;range=[string]$lo.Range.Address($false,$false);header=$(try{[string]$lo.HeaderRowRange.Address($false,$false)}catch{$null});data=$(try{[string]$lo.DataBodyRange.Address($false,$false)}catch{$null})}});pivots=@(foreach($pp in (Items (Call $ws 'PivotTables'))){[string]$pp.Name})}}
                catch{$e[$n]=@{missing=$_.Exception.Message}}
            }
            try{$e.sheets=@(foreach($ws in $script:book.Worksheets){[string]$ws.Name})}catch{}
            return $e
        }
        function Stage5([string]$Name){$script:lightCells=$true;try{Pivots-Read $Name { Extra5 }}finally{$script:lightCells=$false}}
        function Pv5(){$o=$script:book.Worksheets.Item('Out');return ($o.PivotTables('pvt_m5'))}
        function Setup5([string]$Mode){
            $far=$Mode.EndsWith('clean');$Mode=$Mode -replace 'clean$',''
            $wb=$excel.Workbooks.Add(-4167);$script:book=$wb
            $s=$wb.Worksheets.Item(1);$s.Name='Src';$o=$wb.Worksheets.Add([Type]::Missing,$s);$o.Name='Out'
            $h=[object[,]]::new(13,4);$hn='Region','Product','Amount','Units';for($j=0;$j -lt 4;$j++){$h[0,$j]=$hn[$j]}
            $reg='North','South','East';$prd='Alpha','Beta','Gamma','Delta'
            for($i=0;$i -lt 12;$i++){$h[($i+1),0]=$reg[$i%3];$h[($i+1),1]=$prd[$i%4];$h[($i+1),2]=[double](5+$i*10);$h[($i+1),3]=[double](1+$i%5)}
            $s.Range('B2:E14').Value2=$h
            $s.Range('A1').Value2='src-note';$s.Range('H2').Value2='src-far-right';$s.Range('B18').Value2='src-below-marker'
            $src='Src!R2C2:R14C5'
            if($Mode -eq 'table'){$lo=$s.ListObjects.Add(1,$s.Range('B2:E14'),$null,1);$lo.Name='TblSrc';$src='TblSrc'}
            $cache=$wb.PivotCaches().Create(1,$src,8)
            $p=$cache.CreatePivotTable($o.Range('C4'),'pvt_m5',$true,8)
            Row $p 'Region';Col $p 'Product';Add-Data $p 'Amount' 'Sum Amount' -4157
            if($far){$o.Range('A1').Value2='m-a1';$o.Range('A60').Value2='m-far-below';$o.Range('Z5').Value2='m-far-right'}
            else{$o.Range('A1').Value2='m-a1';$o.Range('C2').Value2='m-above';$o.Range('B5').Value2='m-left';$o.Range('C12').Value2='m-below';$o.Range('J5').Value2='m-right'}
            return @{s=$s;o=$o;p=$p;wb=$wb}
        }
        function Scn5([string]$Id,[string]$Mode,[scriptblock]$Act){
            if($Id -notlike $filter){return}
            if($filter -eq '*' -and ($Id -like 'dlg-*' -or $Id -like 'blk-*')){return}
            [void](Try-Step "m5-$Id" {
                $ctx=Setup5 $Mode
                Stage5 "$Id/0-built"
                $ok=Try-Step "$Id/act" { $null = & $Act $ctx.s $ctx.o $ctx.p $ctx.wb }
                Stage5 "$Id/1-after-edit"
                [void](Try-Step "$Id/refresh" { [void](Pv5).RefreshTable() })
                Stage5 "$Id/2-after-refresh"
                $path=Join-Path $dir ("m5-"+$Id+".xlsx")
                $saved=Try-Step "$Id/save" { [void]$script:book.SaveAs($path,51) }
                try{$script:book.Close($false)}catch{}
                $script:book=$null
                if($saved){
                    [void](Try-Step "$Id/reopen" {
                        $script:book=$excel.Workbooks.Open($path,0)
                        Stage5 "$Id/3-reopened"
                        [void](Try-Step "$Id/refresh-reopened" { [void](Pv5).RefreshTable() })
                        Stage5 "$Id/4-reopened-refreshed"
                        $script:book.Close($false);$script:book=$null
                    })
                }
            })
            if($script:book){try{$script:book.Close($false)}catch{};$script:book=$null}
        }

        if($req.watch){
            # Reads, never answers, the text of a dialog Excel raises in the owned process: a thread of this job lists the
            # dialogs (class #32770) of the owned Excel process on this private desktop, with every child text, into
            # dialog.txt. The scenario then runs the one action that raises it and blocks there; the session host's job
            # deadline ends the owned process. Use a short -TimeoutSeconds and one `dlg-` scenario per job.
            Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
public static class DialogWatch {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h, EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  public static void Start(IntPtr excelWindow, string path) {
    uint pid; GetWindowThreadProcessId(excelWindow, out pid);
    var t = new Thread(() => {
      var seen = new List<string>();
      for (int i = 0; i < 2400; i++) {
        var lines = new List<string>();
        EnumWindows((h, l) => {
          uint p; GetWindowThreadProcessId(h, out p);
          if (p != pid) return true;
          var cls = new StringBuilder(256); GetClassName(h, cls, 256);
          if (cls.ToString() != "#32770") return true;
          var title = new StringBuilder(512); GetWindowText(h, title, 512);
          var children = new List<string>();
          EnumChildWindows(h, (c, l2) => { var cc = new StringBuilder(128); GetClassName(c, cc, 128); var tx = new StringBuilder(4096); GetWindowText(c, tx, 4096); children.Add(cc + ": " + tx.ToString().Replace("\r", " ").Replace("\n", " | ")); return true; }, IntPtr.Zero);
          lines.Add("dialog visible=" + IsWindowVisible(h) + " title=" + title + " :: " + string.Join(" // ", children));
          return true;
        }, IntPtr.Zero);
        bool changed = false;
        foreach (var line in lines) if (!seen.Contains(line)) { seen.Add(line); changed = true; }
        if (changed) File.WriteAllLines(path, seen);
        Thread.Sleep(50);
      }
    });
    t.IsBackground = true; t.Start();
  }
}
'@
            [DialogWatch]::Start([IntPtr]$excel.Hwnd,(Join-Path $dir 'dialog.txt'))
        }
        $holder=$book
        $R='range';$T='table';$RC='rangeclean'
        Scn5 'base-range' $R { param($s,$o,$p,$wb) }
        Scn5 'base-table' $T { param($s,$o,$p,$wb) }
        # Rows and columns around the pivot output (Out!C4:H9).
        Scn5 'out-ins-row-above' $R { param($s,$o,$p,$wb) $o.Range('2:2').Insert() | Out-Null }
        Scn5 'out-ins-row-first' $R { param($s,$o,$p,$wb) $o.Range('4:4').Insert() | Out-Null }
        Scn5 'out-ins-row-inside' $R { param($s,$o,$p,$wb) $o.Range('7:7').Insert() | Out-Null }
        Scn5 'out-ins-row-last' $R { param($s,$o,$p,$wb) $o.Range('9:9').Insert() | Out-Null }
        Scn5 'out-ins-row-after' $R { param($s,$o,$p,$wb) $o.Range('10:10').Insert() | Out-Null }
        Scn5 'out-ins-rows-three-inside' $R { param($s,$o,$p,$wb) $o.Range('6:8').Insert() | Out-Null }
        Scn5 'out-ins-cells-partial-down' $R { param($s,$o,$p,$wb) $o.Range('C7:D7').Insert(-4121) | Out-Null }
        Scn5 'out-ins-cells-beside-down' $R { param($s,$o,$p,$wb) $o.Range('I6:J6').Insert(-4121) | Out-Null }
        Scn5 'out-del-row-above' $R { param($s,$o,$p,$wb) $o.Range('1:1').Delete() | Out-Null }
        Scn5 'out-del-row-first' $R { param($s,$o,$p,$wb) $o.Range('4:4').Delete() | Out-Null }
        Scn5 'out-del-row-header2' $R { param($s,$o,$p,$wb) $o.Range('5:5').Delete() | Out-Null }
        Scn5 'out-del-row-inside' $R { param($s,$o,$p,$wb) $o.Range('7:7').Delete() | Out-Null }
        Scn5 'out-del-row-last' $R { param($s,$o,$p,$wb) $o.Range('9:9').Delete() | Out-Null }
        Scn5 'out-del-rows-across-top' $R { param($s,$o,$p,$wb) $o.Range('2:5').Delete() | Out-Null }
        Scn5 'out-del-rows-across-bottom' $R { param($s,$o,$p,$wb) $o.Range('8:11').Delete() | Out-Null }
        Scn5 'out-del-rows-all' $R { param($s,$o,$p,$wb) $o.Range('4:9').Delete() | Out-Null }
        Scn5 'out-del-cells-partial-up' $R { param($s,$o,$p,$wb) $o.Range('C7:D7').Delete(-4162) | Out-Null }
        Scn5 'out-ins-col-before' $R { param($s,$o,$p,$wb) $o.Range('A:A').Insert() | Out-Null }
        Scn5 'out-ins-col-first' $R { param($s,$o,$p,$wb) $o.Range('C:C').Insert() | Out-Null }
        Scn5 'out-ins-col-inside' $R { param($s,$o,$p,$wb) $o.Range('E:E').Insert() | Out-Null }
        Scn5 'out-ins-col-last' $R { param($s,$o,$p,$wb) $o.Range('H:H').Insert() | Out-Null }
        Scn5 'out-ins-col-after' $R { param($s,$o,$p,$wb) $o.Range('I:I').Insert() | Out-Null }
        Scn5 'blk-ins-cells-partial-right' $R { param($s,$o,$p,$wb) $o.Range('E6:E7').Insert(-4161) | Out-Null }
        Scn5 'out-del-col-before' $R { param($s,$o,$p,$wb) $o.Range('A:A').Delete() | Out-Null }
        Scn5 'out-del-col-first' $R { param($s,$o,$p,$wb) $o.Range('C:C').Delete() | Out-Null }
        Scn5 'out-del-col-inside' $R { param($s,$o,$p,$wb) $o.Range('E:E').Delete() | Out-Null }
        Scn5 'out-del-col-last' $R { param($s,$o,$p,$wb) $o.Range('H:H').Delete() | Out-Null }
        Scn5 'out-del-cols-across-left' $R { param($s,$o,$p,$wb) $o.Range('B:D').Delete() | Out-Null }
        Scn5 'out-del-cols-across-right' $R { param($s,$o,$p,$wb) $o.Range('G:J').Delete() | Out-Null }
        Scn5 'out-del-cols-all' $R { param($s,$o,$p,$wb) $o.Range('C:H').Delete() | Out-Null }
        Scn5 'blk-del-cells-partial-left' $R { param($s,$o,$p,$wb) $o.Range('E6:E7').Delete(-4159) | Out-Null }
        Scn5 'out-set-data-cell' $R { param($s,$o,$p,$wb) $o.Range('D7').Value2=[double]1 }
        Scn5 'out-set-label-cell' $R { param($s,$o,$p,$wb) $o.Range('C7').Value2='Renamed' }
        Scn5 'out-clear-all' $R { param($s,$o,$p,$wb) $o.Range('C4:H9').Clear() | Out-Null }
        Scn5 'out-clear-pivot-table' $R { param($s,$o,$p,$wb) $p.TableRange2.Clear() | Out-Null }
        # Worksheet-range source Src!B2:E14 (header row 2, last row 14, fields in B:E, note column A).
        Scn5 'src-ins-row-above' $R { param($s,$o,$p,$wb) $s.Range('1:1').Insert() | Out-Null }
        Scn5 'src-ins-row-header' $R { param($s,$o,$p,$wb) $s.Range('2:2').Insert() | Out-Null }
        Scn5 'src-ins-row-first-data' $R { param($s,$o,$p,$wb) $s.Range('3:3').Insert() | Out-Null }
        Scn5 'src-ins-row-inside' $R { param($s,$o,$p,$wb) $s.Range('8:8').Insert() | Out-Null }
        Scn5 'src-ins-row-last' $R { param($s,$o,$p,$wb) $s.Range('14:14').Insert() | Out-Null }
        Scn5 'src-ins-row-after' $R { param($s,$o,$p,$wb) $s.Range('15:15').Insert() | Out-Null }
        Scn5 'src-ins-cells-partial-down' $R { param($s,$o,$p,$wb) $s.Range('C8:D8').Insert(-4121) | Out-Null }
        Scn5 'src-del-row-above' $R { param($s,$o,$p,$wb) $s.Range('1:1').Delete() | Out-Null }
        Scn5 'src-del-row-header' $R { param($s,$o,$p,$wb) $s.Range('2:2').Delete() | Out-Null }
        Scn5 'src-del-row-first-data' $R { param($s,$o,$p,$wb) $s.Range('3:3').Delete() | Out-Null }
        Scn5 'src-del-row-inside' $R { param($s,$o,$p,$wb) $s.Range('8:8').Delete() | Out-Null }
        Scn5 'src-del-row-last' $R { param($s,$o,$p,$wb) $s.Range('14:14').Delete() | Out-Null }
        Scn5 'src-del-row-after' $R { param($s,$o,$p,$wb) $s.Range('15:15').Delete() | Out-Null }
        Scn5 'src-del-rows-all-data' $R { param($s,$o,$p,$wb) $s.Range('3:14').Delete() | Out-Null }
        Scn5 'src-del-rows-across-end' $R { param($s,$o,$p,$wb) $s.Range('12:16').Delete() | Out-Null }
        Scn5 'src-del-cells-partial-up' $R { param($s,$o,$p,$wb) $s.Range('C8:D8').Delete(-4162) | Out-Null }
        Scn5 'src-ins-col-before' $R { param($s,$o,$p,$wb) $s.Range('A:A').Insert() | Out-Null }
        Scn5 'src-ins-col-first' $R { param($s,$o,$p,$wb) $s.Range('B:B').Insert() | Out-Null }
        Scn5 'src-ins-col-inside' $R { param($s,$o,$p,$wb) $s.Range('D:D').Insert() | Out-Null }
        Scn5 'src-ins-col-last' $R { param($s,$o,$p,$wb) $s.Range('E:E').Insert() | Out-Null }
        Scn5 'src-ins-col-after' $R { param($s,$o,$p,$wb) $s.Range('F:F').Insert() | Out-Null }
        Scn5 'src-del-col-before' $R { param($s,$o,$p,$wb) $s.Range('A:A').Delete() | Out-Null }
        Scn5 'src-del-col-first' $R { param($s,$o,$p,$wb) $s.Range('B:B').Delete() | Out-Null }
        Scn5 'src-del-col-inside-used' $R { param($s,$o,$p,$wb) $s.Range('C:C').Delete() | Out-Null }
        Scn5 'src-del-col-inside-unused' $R { param($s,$o,$p,$wb) $s.Range('E:E').Delete() | Out-Null }
        Scn5 'src-del-col-after' $R { param($s,$o,$p,$wb) $s.Range('F:F').Delete() | Out-Null }
        Scn5 'src-del-cols-all' $R { param($s,$o,$p,$wb) $s.Range('B:E').Delete() | Out-Null }
        Scn5 'src-clear-all' $R { param($s,$o,$p,$wb) $s.Range('B2:E14').Clear() | Out-Null }
        Scn5 'src-edit-header' $R { param($s,$o,$p,$wb) $s.Range('C2').Value2='Item' }
        Scn5 'src-edit-value-then-refresh' $R { param($s,$o,$p,$wb) $s.Range('E3').Value2=[double]1000; $s.Range('D4').Value2='East' }
        # Table source TblSrc (Src!B2:E14).
        Scn5 'tbl-ins-row-header' $T { param($s,$o,$p,$wb) $s.Range('2:2').Insert() | Out-Null }
        Scn5 'tbl-ins-row-first-data' $T { param($s,$o,$p,$wb) $s.Range('3:3').Insert() | Out-Null }
        Scn5 'tbl-ins-row-inside' $T { param($s,$o,$p,$wb) $s.Range('8:8').Insert() | Out-Null }
        Scn5 'tbl-ins-row-last' $T { param($s,$o,$p,$wb) $s.Range('14:14').Insert() | Out-Null }
        Scn5 'tbl-ins-row-after' $T { param($s,$o,$p,$wb) $s.Range('15:15').Insert() | Out-Null }
        Scn5 'tbl-del-row-header' $T { param($s,$o,$p,$wb) $s.Range('2:2').Delete() | Out-Null }
        Scn5 'tbl-del-row-first-data' $T { param($s,$o,$p,$wb) $s.Range('3:3').Delete() | Out-Null }
        Scn5 'tbl-del-row-inside' $T { param($s,$o,$p,$wb) $s.Range('8:8').Delete() | Out-Null }
        Scn5 'tbl-del-row-last' $T { param($s,$o,$p,$wb) $s.Range('14:14').Delete() | Out-Null }
        Scn5 'tbl-del-row-after' $T { param($s,$o,$p,$wb) $s.Range('15:15').Delete() | Out-Null }
        Scn5 'tbl-del-listrow' $T { param($s,$o,$p,$wb) $s.ListObjects.Item('TblSrc').ListRows.Item(5).Delete() | Out-Null }
        Scn5 'tbl-ins-col-before' $T { param($s,$o,$p,$wb) $s.Range('A:A').Insert() | Out-Null }
        Scn5 'tbl-ins-col-first' $T { param($s,$o,$p,$wb) $s.Range('B:B').Insert() | Out-Null }
        Scn5 'tbl-ins-col-inside' $T { param($s,$o,$p,$wb) $s.Range('D:D').Insert() | Out-Null }
        Scn5 'tbl-ins-col-last' $T { param($s,$o,$p,$wb) $s.Range('E:E').Insert() | Out-Null }
        Scn5 'tbl-ins-col-after' $T { param($s,$o,$p,$wb) $s.Range('F:F').Insert() | Out-Null }
        Scn5 'tbl-del-col-first' $T { param($s,$o,$p,$wb) $s.Range('B:B').Delete() | Out-Null }
        Scn5 'tbl-del-col-inside-used' $T { param($s,$o,$p,$wb) $s.Range('C:C').Delete() | Out-Null }
        Scn5 'tbl-del-col-unused' $T { param($s,$o,$p,$wb) $s.Range('E:E').Delete() | Out-Null }
        Scn5 'tbl-del-listcolumn-used' $T { param($s,$o,$p,$wb) $s.ListObjects.Item('TblSrc').ListColumns.Item(2).Delete() | Out-Null }
        Scn5 'tbl-rename-header' $T { param($s,$o,$p,$wb) $s.Range('C2').Value2='Item' }
        # Table growth.
        Scn5 'tbl-add-listrow' $T { param($s,$o,$p,$wb) $lo=$s.ListObjects.Item('TblSrc');$r=$lo.ListRows.Add();$s.Range('B15').Value2='West';$s.Range('C15').Value2='Alpha';$s.Range('D15').Value2=[double]500;$s.Range('E15').Value2=[double]9 }
        Scn5 'tbl-add-listrow-existing-keys' $T { param($s,$o,$p,$wb) $lo=$s.ListObjects.Item('TblSrc');$r=$lo.ListRows.Add();$s.Range('B15').Value2='North';$s.Range('C15').Value2='Alpha';$s.Range('D15').Value2=[double]500;$s.Range('E15').Value2=[double]9 }
        Scn5 'tbl-type-below' $T { param($s,$o,$p,$wb) $s.Range('B15').Value2='West';$s.Range('C15').Value2='Alpha';$s.Range('D15').Value2=[double]500;$s.Range('E15').Value2=[double]9 }
        Scn5 'tbl-type-below-existing-keys' $T { param($s,$o,$p,$wb) $s.Range('B15').Value2='North';$s.Range('C15').Value2='Alpha';$s.Range('D15').Value2=[double]500;$s.Range('E15').Value2=[double]9 }
        Scn5 'tbl-resize-grow' $T { param($s,$o,$p,$wb) $s.Range('B15').Value2='West';$s.Range('C15').Value2='Alpha';$s.Range('D15').Value2=[double]500;$s.Range('E15').Value2=[double]9;$s.ListObjects.Item('TblSrc').Resize($s.Range('B2:E15')) }
        Scn5 'tbl-resize-shrink' $T { param($s,$o,$p,$wb) $s.ListObjects.Item('TblSrc').Resize($s.Range('B2:E10')) }
        Scn5 'tbl-add-listcolumn' $T { param($s,$o,$p,$wb) $lo=$s.ListObjects.Item('TblSrc');$c=$lo.ListColumns.Add() }
        Scn5 'tbl-type-right-header' $T { param($s,$o,$p,$wb) $s.Range('F2').Value2='Extra';$s.Range('F3').Value2=[double]1 }
        Scn5 'tbl-grow-new-product' $T { param($s,$o,$p,$wb) $s.Range('B15').Value2='West';$s.Range('C15').Value2='Epsilon';$s.Range('D15').Value2=[double]500;$s.Range('E15').Value2=[double]9 }
        Scn5 'rng-add-row-below' $R { param($s,$o,$p,$wb) $s.Range('B15').Value2='West';$s.Range('C15').Value2='Alpha';$s.Range('D15').Value2=[double]500;$s.Range('E15').Value2=[double]9 }
        # Overlap on refresh (the output grows into cells): nonblank cells, another pivot, the source.
        Scn5 'ovl-nonblank-right' $R { param($s,$o,$p,$wb) $o.Range('I5').Value2='blocker-I5'; $s.Range('C14').Value2='Epsilon' }
        Scn5 'ovl-nonblank-below' $R { param($s,$o,$p,$wb) $o.Range('C10').Value2='blocker-C10'; $s.Range('B14').Value2='West' }
        Scn5 'ovl-nonblank-below-two' $R { param($s,$o,$p,$wb) $o.Range('C11').Value2='blocker-C11'; $s.Range('B14').Value2='West'; $s.Range('B13').Value2='Far' }
        Scn5 'ovl-nonblank-formula' $R { param($s,$o,$p,$wb) $o.Range('I5').Formula='=1+1'; $s.Range('C14').Value2='Epsilon' }
        Scn5 'ovl-other-pivot-right' $R { param($s,$o,$p,$wb)
            $c2=$wb.PivotCaches().Create(1,'Src!R2C2:R14C5',8);$p2=$c2.CreatePivotTable($o.Range('I4'),'pvt_other',$true,8);Row $p2 'Region';Add-Data $p2 'Units' 'Sum Units' -4157
            $s.Range('C14').Value2='Epsilon' }
        Scn5 'ovl-other-pivot-below' $R { param($s,$o,$p,$wb)
            $c2=$wb.PivotCaches().Create(1,'Src!R2C2:R14C5',8);$p2=$c2.CreatePivotTable($o.Range('C11'),'pvt_other',$true,8);Row $p2 'Region';Add-Data $p2 'Units' 'Sum Units' -4157
            $s.Range('B14').Value2='West' }
        Scn5 'ovl-source-right' $R { param($s,$o,$p,$wb)
            $o.Range('I4:L10').Value2='src';$o.Range('I4').Value2='Region';$o.Range('J4').Value2='Amount'
            $c2=$wb.PivotCaches().Create(1,'Out!R4C9:R10C10',8);$p2=$c2.CreatePivotTable($o.Range('C20'),'pvt_ovsrc',$true,8);Row $p2 'Region';Add-Data $p2 'Amount' 'Count Amount' -4112 }
        Scn5 'ovl-layout-two-rows-into-nonblank' $R { param($s,$o,$p,$wb) $o.Range('C14').Value2='blocker-C14'; Row $p 'Product' 2 }
        Scn5 'ovl-layout-into-nonblank-right' $R { param($s,$o,$p,$wb) $o.Range('K5').Value2='blocker-K5'; Row $p 'Product' 2; Col $p 'Units' 1 }
        Scn5 'ovl-move-location-onto-nonblank' $R { param($s,$o,$p,$wb) $o.Range('M5').Value2='blocker-M5'; $p.Location='Out!$M$4' }
        Scn5 'ovl-move-location-onto-pivot' $R { param($s,$o,$p,$wb)
            $c2=$wb.PivotCaches().Create(1,'Src!R2C2:R14C5',8);$p2=$c2.CreatePivotTable($o.Range('M4'),'pvt_other',$true,8);Row $p2 'Region';Add-Data $p2 'Units' 'Sum Units' -4157
            $p.Location='Out!$M$4' }
        Scn5 'ovl-move-location-free' $R { param($s,$o,$p,$wb) $p.Location='Out!$M$20' }
        Scn5 'ovl-create-onto-nonblank' $R { param($s,$o,$p,$wb) $o.Range('E22').Value2='blocker-E22'; $c2=$wb.PivotCaches().Create(1,'Src!R2C2:R14C5',8);$p2=$c2.CreatePivotTable($o.Range('C21'),'pvt_other',$true,8);Row $p2 'Region';Col $p2 'Product';Add-Data $p2 'Units' 'Sum Units' -4157 }
        Scn5 'ovl-create-onto-pivot' $R { param($s,$o,$p,$wb) $c2=$wb.PivotCaches().Create(1,'Src!R2C2:R14C5',8);$p2=$c2.CreatePivotTable($o.Range('E6'),'pvt_other',$true,8) }
        # Layout changes on an unrefreshed pivot (the source was edited first, no refresh).
        Scn5 'dlg-refresh-overlap-right' $R { param($s,$o,$p,$wb) $o.Range('I5').Value2='blocker-I5'; $s.Range('C14').Value2='Epsilon'; $excel.DisplayAlerts=$true; [void](Pv5).RefreshTable() }
        Scn5 'dlg-refresh-overlap-below' $R { param($s,$o,$p,$wb) $o.Range('C10').Value2='blocker-C10'; $s.Range('B14').Value2='West'; $excel.DisplayAlerts=$true; [void](Pv5).RefreshTable() }
        Scn5 'dlg-layout-overlap' $R { param($s,$o,$p,$wb) $o.Range('C14').Value2='blocker-C14'; $excel.DisplayAlerts=$true; Row $p 'Product' 2 }
        Scn5 'dlg-move-onto-nonblank' $R { param($s,$o,$p,$wb) $o.Range('M5').Value2='blocker-M5'; $excel.DisplayAlerts=$true; $p.Location='Out!$M$4' }
        Scn5 'dlg-move-onto-pivot' $R { param($s,$o,$p,$wb) $c2=$wb.PivotCaches().Create(1,'Src!R2C2:R14C5',8);$p2=$c2.CreatePivotTable($o.Range('M4'),'pvt_other',$true,8);Row $p2 'Region';Add-Data $p2 'Units' 'Sum Units' -4157; $excel.DisplayAlerts=$true; $p.Location='Out!$M$4' }
        Scn5 'dlg-create-onto-nonblank' $R { param($s,$o,$p,$wb) $o.Range('E22').Value2='blocker-E22'; $excel.DisplayAlerts=$true; $c2=$wb.PivotCaches().Create(1,'Src!R2C2:R14C5',8);$p2=$c2.CreatePivotTable($o.Range('C21'),'pvt_other',$true,8);Row $p2 'Region';Col $p2 'Product';Add-Data $p2 'Units' 'Sum Units' -4157 }
        Scn5 'dlg-ins-row-inside' $R { param($s,$o,$p,$wb) $excel.DisplayAlerts=$true; $o.Range('7:7').Insert() | Out-Null }
        Scn5 'dlg-del-row-inside' $R { param($s,$o,$p,$wb) $excel.DisplayAlerts=$true; $o.Range('7:7').Delete() | Out-Null }
        Scn5 'dlg-set-data-cell' $R { param($s,$o,$p,$wb) $excel.DisplayAlerts=$true; $o.Range('D7').Value2=[double]1 }
        Scn5 'dlg-clear-all' $R { param($s,$o,$p,$wb) $excel.DisplayAlerts=$true; $o.Range('C4:H9').Clear() | Out-Null }
        Scn5 'dlg-blk-ins-cells-partial-right' $R { param($s,$o,$p,$wb) $o.Range('E6:E7').Insert(-4161) | Out-Null }
        Scn5 'dlg-blk-del-cells-partial-left' $R { param($s,$o,$p,$wb) $o.Range('E6:E7').Delete(-4159) | Out-Null }
        Scn5 'dlg-source-del-header' $R { param($s,$o,$p,$wb) $excel.DisplayAlerts=$true; $s.Range('2:2').Delete() | Out-Null; [void](Pv5).RefreshTable() }
        Scn5 'dlg-tbl-del-header' $T { param($s,$o,$p,$wb) $excel.DisplayAlerts=$true; $s.Range('2:2').Delete() | Out-Null }
        Scn5 'lay-swap-axes-clean' $RC { param($s,$o,$p,$wb) $p.PivotFields('Region').Orientation=2; $p.PivotFields('Product').Orientation=1 }
        Scn5 'lay-swap-axes-after-source-edit' $RC { param($s,$o,$p,$wb) $s.Range('D3').Value2=[double]99999; $s.Range('C4').Value2='Zeta'; $p.PivotFields('Region').Orientation=2; $p.PivotFields('Product').Orientation=1 }
        Scn5 'lay-add-data-after-source-edit' $RC { param($s,$o,$p,$wb) $s.Range('D3').Value2=[double]99999; Add-Data $p 'Units' 'Sum Units' -4157 }
        Scn5 'lay-move-position-after-source-edit' $RC { param($s,$o,$p,$wb) $s.Range('D3').Value2=[double]99999; $f=$p.PivotFields('Product');$f.Orientation=1;$f.Position=1; $p.PivotFields('Region').Position=2 }
        Scn5 'lay-hide-column-after-source-edit' $RC { param($s,$o,$p,$wb) $s.Range('D3').Value2=[double]99999; $p.PivotFields('Product').Orientation=0 }
        Scn5 'lay-page-field-after-source-edit' $RC { param($s,$o,$p,$wb) $s.Range('D3').Value2=[double]99999; $p.PivotFields('Product').Orientation=3 }
        Scn5 'lay-source-column-added' $RC { param($s,$o,$p,$wb) $s.Range('F2').Value2='Extra';$s.Range('F3').Value2=[double]1; [void]$p.PivotFields('Extra') }
        Scn5 'lay-data-to-row-after-source-edit' $RC { param($s,$o,$p,$wb) $s.Range('D3').Value2=[double]99999; Add-Data $p 'Units' 'Sum Units' -4157; $p.DataPivotField.Orientation=1 }
        Scn5 'lay-new-item-visible-after-source-edit' $RC { param($s,$o,$p,$wb) $s.Range('B3').Value2='West'; Row $p 'Product' 2 }
        Scn5 'lay-clean-add-data' $RC { param($s,$o,$p,$wb) Add-Data $p 'Units' 'Sum Units' -4157 }
        Scn5 'lay-clean-two-rows' $RC { param($s,$o,$p,$wb) Row $p 'Product' 2 }
        Scn5 'lay-clean-products-to-rows-regions-to-cols' $RC { param($s,$o,$p,$wb) $p.PivotFields('Product').Orientation=1;$p.PivotFields('Region').Orientation=2 }
        Scn5 'lay-clean-page-product' $RC { param($s,$o,$p,$wb) $p.PivotFields('Product').Orientation=3 }
        Scn5 'lay-clean-hide-product' $RC { param($s,$o,$p,$wb) $p.PivotFields('Product').Orientation=0 }
        Scn5 'lay-clean-hide-region' $RC { param($s,$o,$p,$wb) $p.PivotFields('Region').Orientation=0 }
        Scn5 'lay-clean-add-units-column' $RC { param($s,$o,$p,$wb) Col $p 'Units' 2 }
        Scn5 'lay-clean-source-edit-then-two-rows' $RC { param($s,$o,$p,$wb) $s.Range('E3').Value2=[double]1000; $s.Range('D4').Value2='West'; Row $p 'Product' 2 }
        $book=$holder
     }
     'open' {
        # Replay of workbooks DNA Calc saved (request `files`, a list of paths): opened read-only in the owned Excel,
        # recording whether Excel repaired the package (new error*.xml logs in %TEMP%, the workbook name),
        # every pivot's cells, every cell of A1:N80 on each sheet, then a refresh of every pivot and the same reads.
        $holder2=$book
        $excel.Calculation=-4105
        function ColL2([int]$n){$s='';while($n -gt 0){$s=[string][char](65+(($n-1)%26))+$s;$n=[int][math]::Floor(($n-1)/26)};return $s}
        function Dump2($ws){
            $l=[Collections.Generic.List[object]]::new()
            $r=$ws.Range('A1:N80');$v=$r.Value2
            for($i=1;$i -le 80;$i++){for($j=1;$j -le 14;$j++){$x=$v[$i,$j];if($null -ne $x){$l.Add(@{sheet=[string]$ws.Name;a=((ColL2 $j)+$i);v=(Enc $x)})}}}
            return ,$l.ToArray()
        }
        $temp=[IO.Path]::GetTempPath()
        foreach($file in @($req.files)){
            $name=[IO.Path]::GetFileNameWithoutExtension([string]$file)
            [void](Try-Step "open-$name" {
                $before=@(Get-ChildItem -LiteralPath $temp -Filter 'error*.xml' -ErrorAction SilentlyContinue|ForEach-Object{$_.FullName})
                $wb=$null
                try{ $wb=$excel.Workbooks.Open([string]$file,0,$true) }
                finally{
                    $after=@(Get-ChildItem -LiteralPath $temp -Filter 'error*.xml' -ErrorAction SilentlyContinue|Where-Object{$before -notcontains $_.FullName})
                    $logs=@(foreach($f in $after){@{file=$f.Name;text=(Get-Content -LiteralPath $f.FullName -Raw)}})
                    $obs.Add(@{stage="$name/open-logs";new_error_logs=$logs;workbook_name=$(if($wb){[string]$wb.Name}else{$null})})
                }
                $script:book=$wb
                $script:lightCells=$true
                Pivots-Read "$name/0-opened" { @{sheets=@(foreach($ws in $wb.Worksheets){@{name=[string]$ws.Name;cells=(Dump2 $ws)}})} }
                foreach($ws in $wb.Worksheets){foreach($p in (Items (Call $ws 'PivotTables'))){[void](Try-Step "$name/refresh-$($p.Name)" {[void]$p.RefreshTable()})}}
                Pivots-Read "$name/1-refreshed" { @{sheets=@(foreach($ws in $wb.Worksheets){@{name=[string]$ws.Name;cells=(Dump2 $ws)}})} }
                $script:lightCells=$false
                $wb.Close($false);$script:book=$null
            })
        }
        $book=$holder2
     }
     'm6' {
        # M6 follow-up questions from the Linux pivot lane (round 4). `scenario` picks one: theme, conflict, model, exact, fonts.
        # Research workbook only: nothing here alters a committed fixture. Answers are data; refusals are retained.
        $excel.Calculation=-4105
        $holder6=$book
        function Rgb6($c){if($null -eq $c){return $null};$v=[int]$c;return ('{0:X2}{1:X2}{2:X2}' -f ($v -band 255),(($v -shr 8) -band 255),(($v -shr 16) -band 255))}
        function Opt6([scriptblock]$Do){try{return (& $Do)}catch{return ('unread: '+$_.Exception.Message)}}
        function Cell6($c){
            $d=$c.DisplayFormat
            $o=[ordered]@{address=[string]$c.Address($false,$false);text=[string]$c.Text;value=(Enc $c.Value2)}
            $o.fill=[ordered]@{color=(Opt6{[int]$d.Interior.Color});rgb=(Opt6{Rgb6 $d.Interior.Color});theme=(Opt6{[int]$d.Interior.ThemeColor});tint=(Opt6{[double]$d.Interior.TintAndShade});pattern=(Opt6{[int]$d.Interior.Pattern})}
            $o.font=[ordered]@{name=(Opt6{[string]$d.Font.Name});size=(Opt6{$d.Font.Size});bold=(Opt6{$d.Font.Bold});color=(Opt6{[int]$d.Font.Color});rgb=(Opt6{Rgb6 $d.Font.Color});theme=(Opt6{[int]$d.Font.ThemeColor});tint=(Opt6{[double]$d.Font.TintAndShade})}
            $b=[ordered]@{}
            foreach($e in @(@('left',7),@('top',8),@('bottom',9),@('right',10))){
                $b[$e[0]]=Opt6{$x=$d.Borders.Item($e[1]);[ordered]@{style=[int]$x.LineStyle;weight=[int]$x.Weight;color=[int]$x.Color;rgb=(Rgb6 $x.Color);theme=(Opt6{[int]$x.ThemeColor});tint=(Opt6{[double]$x.TintAndShade})}}
            }
            $o.borders=$b
            return $o
        }
        function Read6([string]$Stage,$Range){$cells=[Collections.Generic.List[object]]::new();foreach($c in $Range.Cells){$cells.Add((Cell6 $c))};$obs.Add(@{stage=$Stage;utc=[DateTime]::UtcNow.ToString('O');range=[string]$Range.Address($false,$false);cells=$cells.ToArray()})}
        switch([string]$req.scenario){
         'theme' {
            [void](Build-Source $book)
            $SRC='Source!R1C1:R37C5'
            $names=@{light16='PivotStyleLight16';dark11='PivotStyleDark11';medium2='PivotStyleMedium2'}
            $sheets=@{}
            foreach($k in 'light16','dark11','medium2'){
                $style=$names[$k];$kk=$k
                [void](Try-Step "pivot-$k" {
                    $ws=$book.Worksheets.Add([Type]::Missing,$book.Worksheets.Item($book.Worksheets.Count));$ws.Name=$kk
                    $cache=$book.PivotCaches().Create(1,$SRC,8)
                    $p=$cache.CreatePivotTable($ws.Range('A3'),('pvt_'+$kk),$true,8)
                    Row $p 'Region' 1; Row $p 'Product' 2; Add-Data $p 'Amount' 'Sum Amount' -4157; Add-Data $p 'Units' 'Sum Units' -4157; $p.TableStyle2=$style
                    $script:sheets[$kk]=$ws
                })
            }
            $obs.Add(@{stage='theme-colors-office';colors=$(Opt6{$out=@();foreach($i in 1..12){$out+=,@($i,(Rgb6 $book.Theme.ThemeColorScheme.Colors($i).RGB))};$out})})
            function Pass6([string]$Tag){foreach($k in 'light16','dark11','medium2'){$ws=$script:sheets[$k];if($ws){Read6 "$Tag/$k" $ws.Range('A3:C26')}}}
            Pass6 'office'
            [void](Try-Step 'change-accent1' { $book.Theme.ThemeColorScheme.Colors(5).RGB=0x0000C0 })
            Pass6 'accent1-changed'
            [void](Try-Step 'change-accent3' { $book.Theme.ThemeColorScheme.Colors(7).RGB=0x00C000 })
            Pass6 'accent1-and-3-changed'
            $obs.Add(@{stage='theme-colors-changed';colors=$(Opt6{$out=@();foreach($i in 1..12){$out+=,@($i,(Rgb6 $book.Theme.ThemeColorScheme.Colors($i).RGB))};$out})})
            Save 'm6-theme.xlsx'
         }
         'conflict' {
            [void](Build-Source $book)
            $SRC='Source!R1C1:R37C5'
            function Mk6([string]$Id,[bool]$Preserve){
                $ws=$book.Worksheets.Add([Type]::Missing,$book.Worksheets.Item($book.Worksheets.Count));$ws.Name=$Id
                $cache=$book.PivotCaches().Create(1,$SRC,8)
                $p=$cache.CreatePivotTable($ws.Range('A3'),('pvt_'+$Id.Replace('-','_')),$true,8)
                Row $p 'Region'; Col $p 'Product'; Add-Data $p 'Amount' 'Sum Amount' -4157
                $p.PreserveFormatting=$Preserve
                return $p
            }
            $orders=@(@('area-then-cell','area','cell'),@('cell-then-area','cell','area'))
            foreach($o in $orders){
                foreach($preserve in $true,$false){
                    $id=$o[0]+$(if($preserve){'-preserve'}else{'-nopreserve'});$oo=$o;$pp=$preserve
                    [void](Try-Step "case-$id" {
                        $p=Mk6 $id $pp;$ws=$p.Parent
                        $apply={param($kind) if($kind -eq 'area'){ $p.DataBodyRange.Interior.Color=65535;$p.DataBodyRange.Font.Color=255;$p.DataBodyRange.Borders.Item(9).LineStyle=1 } else { $ws.Range('C6').Interior.Color=16711680;$ws.Range('C6').Font.Color=16777215;$ws.Range('C6').Font.Bold=$true } }
                        & $apply $oo[1]; Read6 "$id/1-first" $ws.Range('B5:D7')
                        & $apply $oo[2]; Read6 "$id/2-second" $ws.Range('B5:D7')
                        # The source keys move: North/Beta becomes Nrth/Beta, then Refresh All.
                        $src=$book.Worksheets.Item('Source');$old=$src.Range('A3').Value2
                        $rowNo=0;for($r=2;$r -le 37;$r++){if($src.Cells.Item($r,1).Value2 -eq 'North' -and $src.Cells.Item($r,2).Value2 -eq 'Beta'){$rowNo=$r;break}}
                        $obs.Add(@{stage="$id/source-row";row=$rowNo})
                        $src.Cells.Item($rowNo,1).Value2='Nrth'
                        [void](Try-Step "$id/refresh-all" { $book.RefreshAll();$excel.CalculateUntilAsyncQueriesDone() })
                        Read6 "$id/3-after-refresh-all" $ws.Range('A3:E10')
                        [void](Try-Step "$id/undo" { $excel.Undo() })
                        Read6 "$id/4-after-undo" $ws.Range('A3:E10')
                        $src.Cells.Item($rowNo,1).Value2='North'
                        [void]$p.RefreshTable()
                        Read6 "$id/5-keys-restored" $ws.Range('A3:E10')
                    })
                }
            }
            Save 'm6-conflict.xlsx'
            # Save and reopen: the first case's pivot sheet, read from the saved file.
            [void](Try-Step 'reopen-conflict' {
                Copy-Item -LiteralPath (Join-Path $dir 'm6-conflict.xlsx') -Destination (Join-Path $dir 'm6-conflict-reopen.xlsx');$saved=$excel.Workbooks.Open((Join-Path $dir 'm6-conflict-reopen.xlsx'),0)
                foreach($n in 'area-then-cell-preserve','area-then-cell-nopreserve','cell-then-area-preserve','cell-then-area-nopreserve'){Read6 "$n/6-reopened" $saved.Worksheets.Item($n).Range('A3:E10')}
                $saved.Close($false)
            })
         }
         'fonts' {
            $ws=$book.Worksheets.Item(1);$ws.Name='Fonts'
            $values=@(778.5,2194.75,123456789.125,123463844.625)
            $widths=@(6.29,7.14,8.43,10.57)
            $rows=[Collections.Generic.List[object]]::new()
            foreach($font in 'Aptos Narrow','Courier New'){
                $fn=$font
                $ws.Cells.Clear()|Out-Null
                $ws.Cells.Font.Name=$fn;$ws.Cells.Font.Size=11
                $obs.Add(@{stage="font/$fn";actual=$ws.Range('A1').Font.Name;standard_width=$ws.StandardWidth})
                for($j=0;$j -lt $values.Count;$j++){
                    $v=$values[$j]
                    foreach($w in $widths){
                        $ww=$w
                        $c=$ws.Cells.Item(1,$j+1);$c.Value2=[double]$v;$c.NumberFormat='General'
                        $ws.Columns.Item($j+1).ColumnWidth=$ww
                        $rows.Add([ordered]@{font=$fn;value=$v;set_width=$ww;column_width=$ws.Columns.Item($j+1).ColumnWidth;text=[string]$c.Text;width_px=$ws.Columns.Item($j+1).Width;stage='before-autofit'})
                    }
                    [void]$ws.Columns.Item($j+1).AutoFit()
                    $c=$ws.Cells.Item(1,$j+1)
                    $rows.Add([ordered]@{font=$fn;value=$v;set_width=$null;column_width=$ws.Columns.Item($j+1).ColumnWidth;text=[string]$c.Text;width_px=$ws.Columns.Item($j+1).Width;stage='after-autofit'})
                    # AutoFit starting from each width (AutoFit is independent of the width, recorded once per width).
                    foreach($w in $widths){ $ws.Columns.Item($j+1).ColumnWidth=$w;[void]$ws.Columns.Item($j+1).AutoFit()
                        $rows.Add([ordered]@{font=$fn;value=$v;set_width=$w;column_width=$ws.Columns.Item($j+1).ColumnWidth;text=[string]$c.Text;width_px=$ws.Columns.Item($j+1).Width;stage='autofit-from-width'}) }
                }
            }
            $obs.Add(@{stage='fonts';rows=$rows.ToArray()})
         }
         { $_ -in 'model','exact' } {
            $isExact=([string]$req.scenario -eq 'exact')
            $excel.ScreenUpdating=$true
            $ws=$book.Worksheets.Item(1);$ws.Name='Sheet1'
            $sales=if($isExact){@(@('Region','Amount'),@('East',0.1),@('East',0.2),@('West',0.0000001))}else{@(@('Region','Amount'),@('East',10),@('West',20),@('Unknown',5))}
            for($r=0;$r -lt $sales.Count;$r++){for($c=0;$c -lt 2;$c++){$ws.Cells.Item($r+1,10+$c).Value2=$sales[$r][$c]}}
            $lo=$ws.ListObjects.Add(1,$ws.Range(('J1:K'+$sales.Count)),$null,1);$lo.Name='Sales'
            $ws.Cells.Item(1,13).Value2='Region';$ws.Cells.Item(2,13).Value2='East';$ws.Cells.Item(3,13).Value2='West'
            $lr=$ws.ListObjects.Add(1,$ws.Range('M1:M3'),$null,1);$lr.Name='Regions'
            $modelPath=Join-Path $dir ('m6-'+[string]$req.scenario+'-model.xlsx')
            $book.SaveAs($modelPath,51)
            [void](Try-Step 'connect-sales' { [void]$book.Connections.Add2('WorksheetConnection_Sales','',('WORKSHEET;'+$modelPath),'Sales',7,$true,$false) })
            [void](Try-Step 'connect-regions' { [void]$book.Connections.Add2('WorksheetConnection_Regions','',('WORKSHEET;'+$modelPath),'Regions',7,$true,$false) })
            $model=$book.Model
            $salesT=$null;$regionsT=$null
            [void](Try-Step 'model-tables' { $script:salesT=$model.ModelTables.Item('Sales');$script:regionsT=$model.ModelTables.Item('Regions') })
            $salesT=$script:salesT;$regionsT=$script:regionsT
            foreach($m in @(@('TotalAmount','SUM(Sales[Amount])'),@('AvgAmount','AVERAGE(Sales[Amount])'),@('RowCount','COUNTROWS(Sales)'))){
                $mm=$m;[void](Try-Step ('measure-'+$mm[0]) { [void]$model.ModelMeasures.Add($mm[0],$salesT,$mm[1],$model.ModelFormatGeneral) })
            }
            $out=$ws.Range('A1')
            function CubeRows6([string]$Stage,[string[]]$Formulas){
                $vals=[Collections.Generic.List[object]]::new();$n=0
                foreach($f in $Formulas){
                    $n++;$cell=$ws.Cells.Item($n,3)
                    [void](Try-Step "$Stage/enter-$n" { for($a=1;;$a++){ try{$cell.Formula=$f;break}catch{ if($a -ge 40 -or $_.Exception.HResult -ne -2146777998){throw}; try{$excel.CalculateUntilAsyncQueriesDone()}catch{}; Start-Sleep -Milliseconds 250 } } })
                }
                [void](Try-Step "$Stage/calc" { $excel.CalculateFull();for($i=0;$i -lt 60;$i++){ if([int]$excel.CalculationState -eq 0){break}; Start-Sleep -Milliseconds 500; try{$excel.CalculateUntilAsyncQueriesDone()}catch{} } })
                $n=0;foreach($f in $Formulas){$n++;$cell=$ws.Cells.Item($n,3);$vals.Add([ordered]@{row=$n;formula=[string]$cell.Formula;value=(Enc $cell.Value2);text=[string]$cell.Text})}
                $obs.Add(@{stage=$Stage;utc=[DateTime]::UtcNow.ToString('O');rows=$vals.ToArray()})
            }
            $cv='=CUBEVALUE("ThisWorkbookDataModel","[Measures].[{0}]",{1})'
            if($isExact){
                $f=@(
                  ($cv -f 'TotalAmount','"[Sales].[Region].&[East]"'),($cv -f 'AvgAmount','"[Sales].[Region].&[East]"'),
                  ($cv -f 'TotalAmount','"[Sales].[Region].&[West]"'),($cv -f 'AvgAmount','"[Sales].[Region].&[West]"'),
                  ($cv -f 'TotalAmount',''),($cv -f 'RowCount','"[Sales].[Region].&[East]"'))
                $f=@($f|ForEach-Object{$_ -replace ',\)',')'})
                CubeRows6 'exact/before' $f
                [void](Try-Step 'edit-amount' { $ws.Range('K2').Value2=[double]0.1 })
                [void](Try-Step 'refresh-all' { $book.RefreshAll();$excel.CalculateUntilAsyncQueriesDone() })
                CubeRows6 'exact/after-refresh-all' $f
            } else {
                $f=@(
                  ($cv -f 'TotalAmount','"[Sales].[Region].&[East]"'),($cv -f 'TotalAmount','"[Sales].[Region].&[West]"'),($cv -f 'TotalAmount','"[Sales].[Region].&[Unknown]"'),
                  ($cv -f 'TotalAmount','"[Sales].[Region].&"'),($cv -f 'TotalAmount','"[Sales].[Region].[(blank)]"'),($cv -f 'TotalAmount','"[Sales].[Region].&[]"'),($cv -f 'TotalAmount','"[Sales].[Region].[All]"'),
                  ($cv -f 'TotalAmount','"[Regions].[Region].&[East]"'),($cv -f 'TotalAmount','"[Regions].[Region].&[West]"'),($cv -f 'TotalAmount','"[Regions].[Region].&[]"'),($cv -f 'TotalAmount','"[Regions].[Region].[All]"'),
                  '=CUBEVALUE("ThisWorkbookDataModel","[Measures].[TotalAmount]")')
                CubeRows6 'model/no-relationship/before' $f
                $pv=$book.Worksheets.Add([Type]::Missing,$ws);$pv.Name='ModelPivot'
                function Gpd6([string]$Stage,[string]$Pivot){
                    $g=$book.Worksheets.Item('Gpd');$rows=[Collections.Generic.List[object]]::new()
                    $forms=@(('=GETPIVOTDATA("[Measures].[TotalAmount]",{0}!$A$3)' -f $Pivot),
                      ('=GETPIVOTDATA("[Measures].[TotalAmount]",{0}!$A$3,"[Sales].[Region]","[Sales].[Region].&[East]")' -f $Pivot),
                      ('=GETPIVOTDATA("[Measures].[TotalAmount]",{0}!$A$3,"[Sales].[Region]","[Sales].[Region].&[West]")' -f $Pivot),
                      ('=GETPIVOTDATA("[Measures].[TotalAmount]",{0}!$A$3,"[Sales].[Region]","[Sales].[Region].&[Unknown]")' -f $Pivot),
                      ('=GETPIVOTDATA("[Measures].[TotalAmount]",{0}!$A$3,"[Sales].[Region]","[Sales].[Region].&")' -f $Pivot),
                      ('=GETPIVOTDATA("[Measures].[TotalAmount]",{0}!$A$3,"[Sales].[Region]","")' -f $Pivot))
                    $n=0;foreach($ff in $forms){$n++;$cc=$g.Cells.Item($n,2);$fx=$ff;[void](Try-Step "$Stage/gpd-$n" {$cc.Formula=$fx})}
                    [void](Try-Step "$Stage/calc" { $excel.Calculate();$excel.CalculateUntilAsyncQueriesDone() })
                    $n=0;foreach($ff in $forms){$n++;$cc=$g.Cells.Item($n,2);$rows.Add([ordered]@{row=$n;formula=[string]$cc.Formula;value=(Enc $cc.Value2);text=[string]$cc.Text})}
                    $obs.Add(@{stage=$Stage;rows=$rows.ToArray()})
                }
                $g=$book.Worksheets.Add([Type]::Missing,$pv);$g.Name='Gpd'
                $pt=$null
                [void](Try-Step 'model-pivot' {
                    $cache=$book.PivotCaches().Create(2,$book.Connections.Item('ThisWorkbookDataModel'),8)
                    $script:pt=$cache.CreatePivotTable($pv.Range('A3'),'pvt_model6',$true,8)
                    $script:pt.CubeFields.Item('[Sales].[Region]').Orientation=1
                    $script:pt.CubeFields.Item('[Measures].[TotalAmount]').Orientation=4
                })
                $pt=$script:pt
                [void](Try-Step 'pivot-read' { $obs.Add(@{stage='model/no-relationship/pivot';range=[string]$pt.TableRange2.Address();cells=(Range-Read $pt.TableRange2)}) })
                Gpd6 'model/no-relationship/gpd-before' 'ModelPivot'
                [void](Try-Step 'edit-amount-1' { $ws.Range('K2').Value2=[double]100 })
                [void](Try-Step 'refresh-all-1' { $book.RefreshAll();$excel.CalculateUntilAsyncQueriesDone();$excel.CalculateFull();$excel.CalculateUntilAsyncQueriesDone() })
                CubeRows6 'model/no-relationship/after-refresh-all' $f
                [void](Try-Step 'pivot-read-2' { $obs.Add(@{stage='model/no-relationship/pivot-after';range=[string]$pt.TableRange2.Address();cells=(Range-Read $pt.TableRange2)}) })
                Gpd6 'model/no-relationship/gpd-after' 'ModelPivot'
                # With a relationship Sales[Region] -> Regions[Region]; the edit is undone first so both runs start alike.
                [void](Try-Step 'restore-amount' { $ws.Range('K2').Value2=[double]10 })
                [void](Try-Step 'refresh-all-2' { $book.RefreshAll();$excel.CalculateUntilAsyncQueriesDone() })
                [void](Try-Step 'add-relationship' { [void]$model.ModelRelationships.Add($salesT.ModelTableColumns.Item('Region'),$regionsT.ModelTableColumns.Item('Region')) })
                $obs.Add(@{stage='relationships';list=$(Opt6{Map $model.ModelRelationships {param($r)@{foreign=[string]$r.ForeignKeyColumn.Name;primary=[string]$r.PrimaryKeyColumn.Name;active=$r.Active}}})})
                [void](Try-Step 'refresh-all-3' { $book.RefreshAll();$excel.CalculateUntilAsyncQueriesDone();$excel.CalculateFull();$excel.CalculateUntilAsyncQueriesDone() })
                CubeRows6 'model/relationship/before' $f
                [void](Try-Step 'pivot-read-3' { $obs.Add(@{stage='model/relationship/pivot';range=[string]$pt.TableRange2.Address();cells=(Range-Read $pt.TableRange2)}) })
                Gpd6 'model/relationship/gpd-before' 'ModelPivot'
                [void](Try-Step 'edit-amount-2' { $ws.Range('K2').Value2=[double]100 })
                [void](Try-Step 'refresh-all-4' { $book.RefreshAll();$excel.CalculateUntilAsyncQueriesDone();$excel.CalculateFull();$excel.CalculateUntilAsyncQueriesDone() })
                CubeRows6 'model/relationship/after-refresh-all' $f
                [void](Try-Step 'pivot-read-4' { $obs.Add(@{stage='model/relationship/pivot-after';range=[string]$pt.TableRange2.Address();cells=(Range-Read $pt.TableRange2)}) })
                Gpd6 'model/relationship/gpd-after' 'ModelPivot'
            }
            Save ('m6-'+[string]$req.scenario+'.xlsx')
         }
         default { throw "unknown m6 scenario $($req.scenario)" }
        }
        $book=$holder6
     }
     default {throw "unknown kind $($req.kind)"}
    }
    Assert-HiddenExcel 'after-work'
    $book.Close($false);$book=$null
}catch{$fatal=$_.Exception.ToString()}
finally{if($book){try{$book.Close($false)}catch{}}}
$visibility=Stop-HiddenExcel
if(-not $visibility.compliant){$fatal='Excel visibility changed; this job is invalid'}
$data=@{id=$req.id;kind=$req.kind;scenario=$req.scenario;fatal=$fatal;errors=$errors.ToArray();observations=$obs.ToArray();visibility=$visibility;excel_version=[string]$excel.Version;excel_build=[string]$excel.Build;automation_security_before=$security;automation_security_after=$excel.AutomationSecurity}
[IO.File]::WriteAllText($Result,((ConvertTo-Json -InputObject $data -Depth 40)+"`n"),[Text.UTF8Encoding]::new($false))
if($excel.AutomationSecurity -ne $security){throw 'AutomationSecurity changed'}
