param($OwnedExcel, [string]$Request, [string]$Result)
$ErrorActionPreference='Stop'
$req=Get-Content -LiteralPath $Request -Raw -Encoding utf8|ConvertFrom-Json
$book=$OwnedExcel.Workbooks.Open($req.path,0,$true)
$OwnedExcel.CalculateFull()
$sheet=$book.Worksheets.Item(1)
$rows=@()
for($i=1;$i -le $req.count;$i++){
    $c=$sheet.Range("A$i")
    $o=[ordered]@{at="A$i"}
    try{ $o.formula=$c.Formula2; $o.value2=$c.Value2; $o.text=$c.Text; if($null -ne $c.Value2){$o.type=$c.Value2.GetType().Name} }catch{ $o.error=$_.Exception.Message }
    $rows+=$o
}
$book.Close($false)
@{fatal=$null;rows=$rows}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $Result -Encoding utf8NoBOM
