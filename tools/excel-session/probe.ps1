param($OwnedExcel, [string]$Request, [string]$Result)
$ErrorActionPreference='Stop'
$req=Get-Content -LiteralPath $Request -Raw|ConvertFrom-Json
$book=$OwnedExcel.Workbooks.Add()
$book.Worksheets.Item(1).Range('A1').Formula2='=SUM(1,2,3)'
$OwnedExcel.CalculateFullRebuild()
$value=$book.Worksheets.Item(1).Range('A1').Value2
if ($req.mode -eq 'dirty') {
    $OwnedExcel.Iteration=$true
    $OwnedExcel.MaxIterations=7
    $OwnedExcel.EnableEvents=$true
    # Intentionally leave an open workbook and changed settings for the host to reset.
} elseif ($req.mode -eq 'hang') { Start-Sleep -Seconds 120 }
else { $book.Close($false) }
@{fatal=$null;rows=@(@{value=$value})}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $Result -Encoding utf8NoBOM
