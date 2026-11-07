# One owned-Excel job for OneCalc's session file (dc-0f1k.5): open the workbook OneCalc saved, read
# what Excel sees (the formula as Formula2, whether it is an array formula, the calculated value and
# the constants), list the custom XML parts it holds, then save it as .xlsx under another name so
# the reopen in OneCalc and the custom XML survival can be checked from the file.
# Parameters (named, as submit.ps1 passes them): Source, Output, FormulaCell, Cells (addresses to
# read) and Result (the JSON file this writes). No setting of Excel is changed. Each step records
# its own failure, so a refused call is named in the result.
param($OwnedExcel, [string]$Source, [string]$Output, [string]$FormulaCell, $Cells, [string]$Result)
$ErrorActionPreference='Stop'
$row=[ordered]@{errors=@()}
function Step($name,[scriptblock]$body){ try { & $body } catch { $row.errors+=("$name`: "+$_.Exception.Message) } }
$book=$OwnedExcel.Workbooks.Open($Source)
try {
    Step 'sheets' { $row.sheets=$book.Worksheets.Count }
    $sheet=$book.Worksheets.Item(1)
    Step 'sheet_name' { $row.sheet_name=$sheet.Name }
    $cell=$sheet.Range($FormulaCell)
    Step 'formula' { $row.formula=$cell.Formula }
    Step 'formula2' { $row.formula2=$cell.Formula2 }
    Step 'has_array' { $row.has_array=$cell.HasArray }
    Step 'value2' { $row.value2=$cell.Value2 }
    Step 'text' { $row.text=$cell.Text }
    $values=@{}
    foreach($a in $Cells){ Step "cell $a" { $values[$a]=$sheet.Range($a).Value2 } }
    $row.cells=$values
    $parts=@()
    Step 'custom_xml' { foreach($p in $book.CustomXMLParts){ $parts+=[pscustomobject]@{ns=$p.NamespaceURI;xml=$p.XML} } }
    $row.custom_xml=$parts
    Step 'save_as' { $book.SaveAs($Output,51) }
} finally { Step 'close' { $book.Close($false) } }
@{fatal=$null;rows=@($row)}|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $Result -Encoding utf8NoBOM
