# One owned-Excel job for the look OneCalc saves in its session file (dc-0f1k.20): open the workbook
# read-only, record whether Excel repaired it (new %TEMP%\error*.xml logs), then read what Excel
# shows for the formula's cell and the cells its result fills: the cell format (font, fill, borders,
# alignment), the conditional formats of the range (type, operator, formulas, text, rank, average,
# colours, thresholds, bar and icon settings) and each cell's DisplayFormat, which includes what the
# rules give it. Then it saves the workbook as .xlsx under another name so the reopen in OneCalc can
# be checked from the file.
# Parameters (named, as submit.ps1 passes them): Source, Output, Range (the result's A1 range),
# Result (the JSON file this writes). No setting of Excel is changed and no dialog is answered.
# Each read records its own failure.
param($OwnedExcel, [string]$Source, [string]$Output, [string]$Range, [string]$Result)
$ErrorActionPreference='Stop'
$row=[ordered]@{errors=@()}
function Step($name,[scriptblock]$body){ try { & $body } catch { $row.errors+=("$name`: "+$_.Exception.Message) } }
function Try-Read([scriptblock]$body){ try { return (& $body) } catch { return @{unread=$_.Exception.Message} } }
$temp=[IO.Path]::GetTempPath()
$before=@(Get-ChildItem -LiteralPath $temp -Filter 'error*.xml' -ErrorAction SilentlyContinue|ForEach-Object{$_.FullName})
$book=$null
try { $book=$OwnedExcel.Workbooks.Open($Source,0,$true) }
finally {
    $after=@(Get-ChildItem -LiteralPath $temp -Filter 'error*.xml' -ErrorAction SilentlyContinue|Where-Object{$before -notcontains $_.FullName})
    $row.new_error_logs=@(foreach($f in $after){@{file=$f.Name;text=(Get-Content -LiteralPath $f.FullName -Raw)}})
    $row.workbook_name=$(if($book){[string]$book.Name}else{$null})
}
try {
    $sheet=$book.Worksheets.Item(1)
    $area=$sheet.Range($Range)
    $row.range=[string]$area.Address($false,$false)
    function Edge($c,[int]$i){ Try-Read { @{style=[int]$c.Borders.Item($i).LineStyle;color=[int]$c.Borders.Item($i).Color;weight=[int]$c.Borders.Item($i).Weight} } }
    $cells=@()
    foreach($c in $area.Cells){
        $d=Try-Read { $c.DisplayFormat }
        $cells+=[ordered]@{
            address=[string]$c.Address($false,$false)
            formula=[string]$c.Formula2
            value2=Try-Read { $c.Value2 }
            text=[string]$c.Text
            font=[ordered]@{name=[string]$c.Font.Name;size=[double]$c.Font.Size;bold=[bool]$c.Font.Bold;italic=[bool]$c.Font.Italic;underline=[int]$c.Font.Underline;strike=[bool]$c.Font.Strikethrough;color=[int]$c.Font.Color}
            interior=[ordered]@{color=[int]$c.Interior.Color;pattern=[int]$c.Interior.Pattern}
            borders=[ordered]@{left=(Edge $c 7);top=(Edge $c 8);bottom=(Edge $c 9);right=(Edge $c 10)}
            horizontal=[int]$c.HorizontalAlignment
            wrap=[bool]$c.WrapText
            number_format=[string]$c.NumberFormat
            display=Try-Read { [ordered]@{font_color=[int]$d.Font.Color;bold=[bool]$d.Font.Bold;italic=[bool]$d.Font.Italic;underline=[int]$d.Font.Underline;strike=[bool]$d.Font.Strikethrough;interior_color=[int]$d.Interior.Color;interior_pattern=[int]$d.Interior.Pattern;number_format=[string]$d.NumberFormat} }
        }
    }
    $row.cells=$cells
    $conds=@()
    $fc=$area.FormatConditions
    $row.condition_count=[int]$fc.Count
    for($i=1;$i -le [int]$fc.Count;$i++){
        $f=$fc.Item($i)
        $o=[ordered]@{index=$i}
        foreach($p in 'Type','Priority','StopIfTrue','Operator','Formula1','Formula2','Text','TextOperator','DateOperator','Rank','Percent','TopBottom','AboveBelow','NumStdDev','NumberFormat','ShowValue','ReverseOrder','ShowIconOnly','BarFillType','Direction'){
            $o[$p]=Try-Read { $v=$f.$p; if($v -is [string] -or $v -is [bool]){$v}else{[int]$v} }
        }
        $o.applies_to=Try-Read { [string]$f.AppliesTo.Address($false,$false) }
        $o.font=Try-Read { [ordered]@{color=[int]$f.Font.Color;bold=$f.Font.Bold;italic=$f.Font.Italic;underline=$f.Font.Underline;strike=$f.Font.Strikethrough} }
        $o.interior=Try-Read { [ordered]@{color=[int]$f.Interior.Color;pattern=[int]$f.Interior.Pattern} }
        $o.criteria=Try-Read { @(foreach($k in 1..$f.ColorScaleCriteria.Count){$cr=$f.ColorScaleCriteria.Item($k);@{type=[int]$cr.Type;value=$cr.Value;color=[int]$cr.FormatColor.Color}}) }
        $o.bar=Try-Read { [ordered]@{color=[int]$f.BarColor.Color;min_type=[int]$f.MinPoint.Type;max_type=[int]$f.MaxPoint.Type;fill=[int]$f.BarFillType;negative=$null} }
        $o.icons=Try-Read { [ordered]@{id=[int]$f.IconSet.ID;count=[int]$f.IconCriteria.Count;types=@(foreach($k in 1..$f.IconCriteria.Count){$ic=$f.IconCriteria.Item($k);@{type=[int]$ic.Type;value=$ic.Value;operator=[int]$ic.Operator}})} }
        $conds+=$o
    }
    $row.conditions=$conds
    Step 'save_as' { $book.SaveAs($Output,51) }
} finally { Step 'close' { $book.Close($false) } }
@{fatal=$null;rows=@($row)}|ConvertTo-Json -Depth 10|Set-Content -LiteralPath $Result -Encoding utf8NoBOM
