param($OwnedExcel, [string]$Request, [string]$Result)
$ErrorActionPreference='Stop'
$req=Get-Content -LiteralPath $Request -Raw -Encoding utf8|ConvertFrom-Json
$book=$OwnedExcel.Workbooks.Add()
$sheet=$book.Worksheets.Item(1)
$rows=@()
$entry=@{}
$n=0
foreach($c in $req.cells){
    $n++
    $addr=$c.at
    $rec=[ordered]@{at=$addr;formula=$c.f}
    try{
        if($c.PSObject.Properties['text']){ $sheet.Range($addr).NumberFormat='@'; $sheet.Range($addr).Value2=[string]$c.text }
        elseif($c.PSObject.Properties['num']){ $sheet.Range($addr).Value2=[double]$c.num }
        else{
            if($c.PSObject.Properties['legacy'] -and $c.legacy){ $sheet.Range($addr).Formula=$c.f } else { $sheet.Range($addr).Formula2=$c.f }
        }
        $entry[$addr]=@{entered=$true}
    }catch{ $entry[$addr]=@{entered=$false;error=$_.Exception.Message} }
}
$OwnedExcel.CalculateFull()
$deadline=[DateTime]::UtcNow.AddSeconds(25)
while([DateTime]::UtcNow -lt $deadline){
    $busy=$false
    foreach($c in $req.cells){ try{ $v=$sheet.Range($c.at).Value2; if($v -is [int] -and $v -eq -2146826237){$busy=$true} }catch{} }
    if(-not $busy){break}
    try{ $OwnedExcel.CalculateUntilAsyncQueriesDone() }catch{}
    Start-Sleep -Milliseconds 300
}
foreach($c in $req.cells){
    $addr=$c.at
    $cell=$sheet.Range($addr)
    $out=@{at=$addr;formula=$c.f;entry=$entry[$addr]}
    try{
        $v=$cell.Value2
        $out.value2=$v
        $out.type=if($null -eq $v){'null'}else{$v.GetType().Name}
        $out.text=$cell.Text
        $out.formula_back=$cell.Formula2
        if($v -is [int] -and $cell.Text -match '^#'){ $out.error_code=$v }
    }catch{ $out.read_error=$_.Exception.Message }
    $rows+=$out
}
$book.Close($false)
@{fatal=$null;rows=$rows}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $Result -Encoding utf8NoBOM
