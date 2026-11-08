param($OwnedExcel, [string]$Request, [string]$Result)
$ErrorActionPreference='Stop'
$req=Get-Content -LiteralPath $Request -Raw -Encoding utf8|ConvertFrom-Json
$book=$OwnedExcel.Workbooks.Add()
$sheet=$book.Worksheets.Item(1)
$macro=$null
$rows=@()
function Describe($v){ if($null -eq $v){return @{value=$null;type='null'}}; $t=$v.GetType().Name; if($v -is [int] -and $v -lt 0){ return @{value=$v;type='errorint'} }; return @{value=$v;type=$t} }
foreach($c in $req.cells){
    $out=[ordered]@{id=$c.id;mode=$c.mode;f=$c.f}
    try{
        switch($c.mode){
            'legacy' { $sheet.Range($c.at).Formula=$c.f; $OwnedExcel.CalculateFull(); $out.value2=$sheet.Range($c.at).Value2; $out.text=$sheet.Range($c.at).Text; $out.back=$sheet.Range($c.at).Formula }
            'formula2' { $sheet.Range($c.at).Formula2=$c.f; $OwnedExcel.CalculateFull(); $out.value2=$sheet.Range($c.at).Value2; $out.text=$sheet.Range($c.at).Text }
            'evaluate' { $v=$sheet.Evaluate($c.f); $out.value2=$v; $out.type=if($null -eq $v){'null'}else{$v.GetType().Name} }
            'app_evaluate' { $v=$OwnedExcel.Evaluate($c.f); $out.value2=$v; $out.type=if($null -eq $v){'null'}else{$v.GetType().Name} }
            'xlm' { $v=$OwnedExcel.ExecuteExcel4Macro($c.f); $out.value2=$v; $out.type=if($null -eq $v){'null'}else{$v.GetType().Name} }
            'name' { $nm=$book.Names.Add($c.name,$c.f); $sheet.Range($c.at).Formula2=('='+$c.name); $OwnedExcel.CalculateFull(); $out.value2=$sheet.Range($c.at).Value2; $out.text=$sheet.Range($c.at).Text; $out.refers=$nm.RefersTo }
            'macrosheet' {
                if($null -eq $macro){ $macro=$book.Sheets.Add([Type]::Missing,[Type]::Missing,1,4) ; $macro.Name='M1' }
                $macro.Range($c.at).Formula=$c.f; $OwnedExcel.CalculateFull(); $out.value2=$macro.Range($c.at).Value2; $out.text=$macro.Range($c.at).Text; $out.back=$macro.Range($c.at).Formula
            }
        }
        if($out.value2 -is [int]){ $out.type='int' }
        elseif($null -ne $out.value2 -and -not $out.type){ $out.type=$out.value2.GetType().Name }
    }catch{ $out.error=$_.Exception.Message }
    $rows+=$out
}
$OwnedExcel.DisplayAlerts=$false
$book.Close($false)
@{fatal=$null;rows=$rows}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $Result -Encoding utf8NoBOM
