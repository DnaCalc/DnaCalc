param($OwnedExcel, [string]$Request, [string]$Result)
$ErrorActionPreference='Stop'
$req=Get-Content -LiteralPath $Request -Raw -Encoding utf8|ConvertFrom-Json
$book=$OwnedExcel.Workbooks.Add()
$sheet=$book.Worksheets.Item(1)
$entry=@{}
$i=0
foreach($c in $req.cells){
    $col=1+14*$i
    $anchor=$sheet.Cells.Item(1,$col)
    try{ $anchor.Formula2=$c.f; $entry[$c.id]=@{entered=$true;at=$anchor.Address($false,$false)} }catch{ $entry[$c.id]=@{entered=$false;error=$_.Exception.Message} }
    $c | Add-Member -NotePropertyName anchorCol -NotePropertyValue $col -Force
    $i++
}
$OwnedExcel.CalculateFull()
$deadline=[DateTime]::UtcNow.AddSeconds(40)
while([DateTime]::UtcNow -lt $deadline){
    $busy=$false
    foreach($c in $req.cells){ try{ $v=$sheet.Cells.Item(1,$c.anchorCol).Value2; if($v -is [int] -and $v -eq -2146826237){$busy=$true} }catch{} }
    if(-not $busy){break}
    try{ $OwnedExcel.CalculateUntilAsyncQueriesDone() }catch{}
    Start-Sleep -Milliseconds 400
}
function Cell($x){
    if($null -eq $x){ return @('empty',$null) }
    if($x -is [int]){ return @('error',$x) }
    if($x -is [double]){ return @('number',$x) }
    if($x -is [bool]){ return @('logical',$x) }
    if($x -is [string]){ return @('text',$x) }
    return @($x.GetType().Name,"$x")
}
$rows=@()
foreach($c in $req.cells){
    $anchor=$sheet.Cells.Item(1,$c.anchorCol)
    $o=[ordered]@{id=$c.id;f=$c.f;entry=$entry[$c.id]}
    try{
        $o.anchor_text=$anchor.Text
        $spill=$null
        try{ $spill=$anchor.SpillingToRange }catch{}
        $o.spill_address=$null
        $cells=@()
        if($spill -ne $null){
            $o.spill_address=$spill.Address($false,$false)
            $nr=$spill.Rows.Count; $nc=$spill.Columns.Count
            $o.rows=$nr; $o.cols=$nc
            for($r=1;$r -le $nr;$r++){ $line=@(); for($k=1;$k -le $nc;$k++){ $cell=$spill.Cells.Item($r,$k); $t=Cell $cell.Value2; $line+=,@($t[0],$t[1],$cell.Text,$cell.NumberFormat) }; $cells+=,$line }
        } else {
            $t=Cell $anchor.Value2; $cells=@(,@(,@($t[0],$t[1],$anchor.Text,$anchor.NumberFormat)))
            $o.rows=1;$o.cols=1
        }
        $o.cells=$cells
    }catch{ $o.error=$_.Exception.Message }
    $rows+=$o
}
$book.Close($false)
@{fatal=$null;rows=$rows}|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $Result -Encoding utf8NoBOM
