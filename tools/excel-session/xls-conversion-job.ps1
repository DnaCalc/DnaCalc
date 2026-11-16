# dc-4j0e.12: Excel conversion observations for controlled fixtures written by the calc-doc test.
# One owned Excel session; Result contains observations and each saved package as base64.
param($OwnedExcel, [string]$Cases, [string]$Result)
$ErrorActionPreference = 'Stop'
$app = $OwnedExcel
$casesPath = [IO.Path]::GetFullPath($Cases)
$cases = Get-Content -LiteralPath $casesPath -Raw -Encoding utf8 | ConvertFrom-Json
$inputDir = [IO.Path]::GetDirectoryName($casesPath)
$outputDir = Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($Result))) ('xls-conversion-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($outputDir) | Out-Null
$rows = [Collections.Generic.List[object]]::new()
$book = $null; $fatal = $null; $pivot = $null; $oldAlerts = $app.DisplayAlerts
function GetOrError([scriptblock]$f) { try { return & $f } catch { return @{unread=$_.Exception.Message} } }
function Snapshot($workbook) {
    $sheets = [Collections.Generic.List[object]]::new()
    foreach ($sheet in $workbook.Sheets) {
        $cells = [Collections.Generic.List[object]]::new()
        $used = $sheet.UsedRange
        # Inputs are generated fixtures. Refuse unexpectedly large ranges rather than producing
        # an incomplete list without saying so.
        if ([double]$used.Cells.CountLarge -gt 100000) { throw "Used range too large: $($sheet.Name)" }
        foreach ($cell in $used.Cells) {
            if ($null -ne $cell.Value2 -or $cell.HasFormula -or $cell.MergeCells -or $null -ne $cell.Comment) {
                $cells.Add(@{address=[string]$cell.Address($false,$false); value=$(if ($null -eq $cell.Value2) {$null} else {[string]$cell.Value2});
                    formula=$(if ($cell.HasFormula) {[string]$cell.Formula} else {$null}); number_format=[string]$cell.NumberFormat;
                    merge=$(if ($cell.MergeCells) {[string]$cell.MergeArea.Address($false,$false)} else {$null});
                    comment=$(if ($null -ne $cell.Comment) {[string]$cell.Comment.Text()} else {$null})})
            }
        }
        $links = @($sheet.Hyperlinks | ForEach-Object { @{address=[string]$_.Range.Address($false,$false); target=[string]$_.Address; location=[string]$_.SubAddress} })
        $shapes = @($sheet.Shapes | ForEach-Object { @{name=[string]$_.Name; type=$_.Type; top_left=(GetOrError { [string]$_.TopLeftCell.Address($false,$false) }); bottom_right=(GetOrError { [string]$_.BottomRightCell.Address($false,$false) })} })
        $sheets.Add(@{name=[string]$sheet.Name; visible=$sheet.Visible; cells=$cells.ToArray(); links=$links; shapes=$shapes;
            conditional_formats=(GetOrError {$used.FormatConditions.Count}); validations=(GetOrError {$used.SpecialCells(-4174).CountLarge});
            charts=(GetOrError {$sheet.ChartObjects().Count})})
    }
    $names = @($workbook.Names | ForEach-Object { @{name=[string]$_.Name; refers_to=[string]$_.RefersTo; visible=$_.Visible} })
    return @{date1904=$workbook.Date1904; sheets=$sheets.ToArray(); names=$names; file_format=$workbook.FileFormat}
}
try {
    $app.DisplayAlerts = $false
    foreach ($name in $cases.files) {
        $name = [string]$name
        if ([IO.Path]::GetFileName($name) -ne $name) { throw "Input must be a fixture basename: $name" }
        $input = Join-Path $inputDir $name
        $entry = [ordered]@{file=$name; sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $input).Hash.ToLowerInvariant(); error=$null}
        try {
            $book = $app.Workbooks.Open($input, 0, $true)
            $entry.before = Snapshot $book
            $hasVba = [bool]$book.HasVBProject
            $extension = if ($hasVba) {'.xlsm'} else {'.xlsx'}
            $format = if ($hasVba) {52} else {51}
            $saved = Join-Path $outputDir ([IO.Path]::GetFileNameWithoutExtension($name) + $extension)
            $book.SaveAs($saved, $format)
            $entry.after = Snapshot $book
            $book.Close($false); $book = $null
            $entry.package_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($saved))
        } catch { $entry.error = $_.Exception.ToString() }
        finally { if ($book) { try {$book.Close($false)} catch {}; $book=$null } }
        $rows.Add($entry)
    }
    if ($cases.create_pivot) {
        $pivot = [ordered]@{error=$null}
        try {
            $book = $app.Workbooks.Add(-4167)
            $data = $book.Worksheets.Item(1); $data.Name = 'Data'
            $out = $book.Worksheets.Add([Type]::Missing, $data); $out.Name = 'Pivot'
            $values = [object[,]]::new(4,2)
            $values[0,0]='Region'; $values[0,1]='Amount'
            $values[1,0]='North'; $values[1,1]=2.0
            $values[2,0]='South'; $values[2,1]=3.0
            $values[3,0]='North'; $values[3,1]=5.0
            $data.Range('A1:B4').Value2 = $values
            $cache = $book.PivotCaches().Create(1, 'Data!R1C1:R4C2', 1)
            $table = $cache.CreatePivotTable($out.Range('A3'), 'LegacyPivot', $true, 1)
            $table.PivotFields('Region').Orientation = 1
            $null = $table.AddDataField($table.PivotFields('Amount'), 'Sum of Amount', -4157)
            $xls = Join-Path $outputDir 'pivot.xls'
            $xlsx = Join-Path $outputDir 'pivot.xlsx'
            $book.SaveAs($xls, 56)
            $book.Close($false); $book = $null
            $pivot.xls_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($xls))
            $book = $app.Workbooks.Open($xls, 0, $false)
            $pivot.before = Snapshot $book
            $book.SaveAs($xlsx, 51)
            $book.Close($false); $book = $null
            $pivot.xlsx_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($xlsx))
        } catch { $pivot.error = $_.Exception.ToString() }
        finally { if ($book) { try {$book.Close($false)} catch {}; $book=$null } }
    }
} catch { $fatal = $_.Exception.ToString() }
finally { if ($book) { try {$book.Close($false)} catch {} }; $app.DisplayAlerts = $oldAlerts }
@{fatal=$fatal; excel_version=[string]$app.Version; excel_build=[string]$app.Build; files=$rows.ToArray(); pivot=$pivot} |
    ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $Result -Encoding utf8NoBOM
