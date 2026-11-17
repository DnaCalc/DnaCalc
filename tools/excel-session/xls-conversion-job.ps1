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
$generated = [Collections.Generic.List[object]]::new()
$repairs = [Collections.Generic.List[object]]::new()
$book = $null; $fatal = $null; $pivot = $null; $oldAlerts = $app.DisplayAlerts
function GetOrError([scriptblock]$f) { try { return & $f } catch { return @{unread=$_.Exception.Message} } }
function Snapshot($workbook) {
    $sheets = [Collections.Generic.List[object]]::new()
    foreach ($sheet in $workbook.Sheets) {
        # A chart sheet is a Chart, not a Worksheet, and has no UsedRange or Hyperlinks.
        $chartSheet = $null
        try { $chartSheet = $workbook.Charts.Item([string]$sheet.Name) } catch { }
        if ($null -ne $chartSheet) {
            $chart = GetOrError {
                @{type=$chartSheet.ChartType; title=$(if ($chartSheet.HasTitle) {[string]$chartSheet.ChartTitle.Text} else {$null});
                  series=@($chartSheet.SeriesCollection() | ForEach-Object { [string]$_.Formula });
                  chart_size=(GetOrError { $chartSheet.PageSetup.ChartSize })}
            }
            $sheets.Add(@{name=[string]$sheet.Name; visible=$sheet.Visible; chart_sheet=$chart;
                cells=@(); links=@(); shapes=@(); conditional_formats=$null; validations=$null; charts=$null})
            continue
        }
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
    # Create fresh, macro-free fixtures in Excel, then reopen each legacy file before conversion.
    # FileFormat 18 is an add-in and is saved as an Open XML add-in (55).
    foreach ($fixture in @(
        @{name='excel-biff8.xls'; legacy=56; modern=51; ext='.xlsx'},
        @{name='excel-biff5.xls'; legacy=39; modern=51; ext='.xlsx'},
        @{name='excel-addin.xla'; legacy=18; modern=55; ext='.xlam'}
    )) {
        $entry = [ordered]@{file=$fixture.name; error=$null}
        try {
            $path = Join-Path $outputDir $fixture.name
            $saved = Join-Path $outputDir ([IO.Path]::GetFileNameWithoutExtension($fixture.name) + $fixture.ext)
            $book = $app.Workbooks.Add(-4167)
            $sheet = $book.Worksheets.Item(1); $sheet.Name = 'Data'
            $sheet.Range('A1').Value2 = 42.5
            $sheet.Range('A2').Formula = '=A1+1'
            $sheet.Range('A1').NumberFormat = '0.00'
            # Record setup failures individually; a format may refuse particular features.
            $entry.setup = [ordered]@{}
            $entry.setup.document_properties = GetOrError {
                $properties = $book.BuiltinDocumentProperties
                $properties.Item('Title').Value = 'Controlled title'
                $properties.Item('Subject').Value = 'Controlled subject'
                $properties.Item('Author').Value = 'Controlled author'
                $properties.Item('Keywords').Value = 'legacy, controlled'
                $properties.Item('Comments').Value = 'Controlled description'
                @{title=[string]$properties.Item('Title').Value; author=[string]$properties.Item('Author').Value}
            }
            $entry.setup.document_properties_extended = GetOrError {
                $properties = $book.BuiltinDocumentProperties
                $properties.Item('Category').Value = 'Controlled category'
                $properties.Item('Manager').Value = 'Controlled manager'
                $properties.Item('Company').Value = 'Controlled company'
                @{category=[string]$properties.Item('Category').Value; company=[string]$properties.Item('Company').Value}
            }
            $entry.setup.document_properties_custom = GetOrError {
                $properties = $book.CustomDocumentProperties
                $null = $properties.Add('cxText', $false, 4, 'Controlled custom value')
                $null = $properties.Add('cxCount', $false, 1, 42)
                $null = $properties.Add('cxApproved', $false, 2, $true)
                @{count=$properties.Count; text=[string]$properties.Item('cxText').Value}
            }
            $entry.setup.name = GetOrError { $n = $book.Names.Add('cx_rate', '=Data!$A$1'); [string]$n.RefersTo }
            $entry.setup.merge = GetOrError { $sheet.Range('C1:D1').Merge(); [string]$sheet.Range('C1').MergeArea.Address($false,$false) }
            $entry.setup.hyperlink = GetOrError { $h = $sheet.Hyperlinks.Add($sheet.Range('C2'), 'https://example.com/'); [string]$h.Address }
            $entry.setup.relative_hyperlink = GetOrError { $h = $sheet.Hyperlinks.Add($sheet.Range('C6'), '..\folder\book.xls', 'Data!A1'); @{address=[string]$h.Address; location=[string]$h.SubAddress} }
            $entry.setup.comment = GetOrError { $c = $sheet.Range('C3').AddComment('Controlled note'); [string]$c.Text() }
            $entry.setup.validation = GetOrError { $sheet.Range('C4').Validation.Add(1, 1, 1, '1', '10'); $sheet.Range('C4').Validation.Type }
            $entry.setup.active_filter = GetOrError {
                $sheet.Range('K1').Value2 = 'Group'; $sheet.Range('K2').Value2 = 'keep'; $sheet.Range('K3').Value2 = 'drop'
                $null = $sheet.Range('K1:K3').AutoFilter(1, 'keep')
                @{filter_mode=$sheet.FilterMode; hidden=$sheet.Rows.Item(3).Hidden}
            }
            $entry.setup.conditional = GetOrError { $c = $sheet.Range('C5').FormatConditions.Add(1, 3, '1'); $c.Interior.Color = 65535; $sheet.Range('C5').FormatConditions.Count }
            $entry.setup.scenario = GetOrError {
                $scenario = $sheet.Scenarios().Add('cxPlan', $sheet.Range('A1'), [object[]]@(42), 'Controlled scenario', $true, $false)
                @{name=[string]$scenario.Name; comment=[string]$scenario.Comment; count=$sheet.Scenarios().Count}
            }
            $entry.setup.shape = GetOrError { $s = $sheet.Shapes.AddShape(1, 100, 100, 60, 30); [string]$s.Name }
            $entry.setup.chart_bar_geometry = GetOrError {
                $sheet.Range('H1').Value2 = 1; $sheet.Range('H2').Value2 = 2
                $c = $sheet.ChartObjects().Add(200, 100, 200, 150).Chart
                $c.ChartType = 51; $c.SetSourceData($sheet.Range('H1:H2'))
                $c.PlotVisibleOnly = $false; $c.DisplayBlanksAs = 3
                $group = $c.ChartGroups(1); $group.GapWidth = 70; $group.Overlap = -30; $group.VaryByCategories = $true
                $axis = $c.Axes(2); $axis.MinimumScale = 2; $axis.MaximumScale = 20
                $axis.MajorUnit = 5; $axis.MinorUnit = 1; $axis.ReversePlotOrder = $true
                @{gap=$group.GapWidth; overlap=$group.Overlap; varied=$group.VaryByCategories;
                  visible_only=$c.PlotVisibleOnly; display_blanks=$c.DisplayBlanksAs;
                  minimum=$axis.MinimumScale; maximum=$axis.MaximumScale; major=$axis.MajorUnit;
                  minor=$axis.MinorUnit; reversed=$axis.ReversePlotOrder}
            }
            $entry.setup.chart_print_text = GetOrError {
                $chart = $sheet.ChartObjects().Item(1).Chart
                $chart.PageSetup.CenterHeader = 'Controlled chart header'
                $chart.PageSetup.RightFooter = 'Page &P'
                @{header=[string]$chart.PageSetup.CenterHeader; footer=[string]$chart.PageSetup.RightFooter}
            }
            $entry.setup.chart_doughnut_geometry = GetOrError {
                $c = $sheet.ChartObjects().Add(200, 270, 200, 150).Chart
                $c.ChartType = -4120; $c.SetSourceData($sheet.Range('H1:H2'))
                $group = $c.ChartGroups(1); $group.FirstSliceAngle = 135; $group.DoughnutHoleSize = 45
                @{angle=$group.FirstSliceAngle; hole=$group.DoughnutHoleSize}
            }
            $entry.setup.chart_cross_sheet_cache = GetOrError {
                $dataSheet = $book.Worksheets.Add(); $dataSheet.Name = 'ChartSource'
                $dataSheet.Range('A1').Value2 = 'Category'; $dataSheet.Range('B1').Value2 = 'Value'
                $dataSheet.Range('A2').Value2 = 'one'; $dataSheet.Range('A3').Value2 = 'two'
                $dataSheet.Range('B2').Value2 = 7.5; $dataSheet.Range('B3').Value2 = 4
                $object = $sheet.ChartObjects().Add(410, 100, 200, 150); $object.Name = 'cxCacheChart'
                $c = $object.Chart
                $c.ChartType = 51; $c.SetSourceData($dataSheet.Range('A1:B3'))
                @{series=[string]$c.SeriesCollection(1).Formula; points=$c.SeriesCollection(1).Points().Count}
            }
            $entry.setup.chart_series_fill = GetOrError {
                $series = $sheet.ChartObjects('cxCacheChart').Chart.SeriesCollection(1)
                $series.Format.Fill.Visible = -1; $series.Format.Fill.Solid()
                $series.Format.Fill.ForeColor.RGB = 255
                @{fill_rgb=$series.Format.Fill.ForeColor.RGB}
            }
            $entry.setup.chart_series_line = GetOrError {
                $chart = $sheet.ChartObjects().Add(410, 270, 200, 150).Chart
                $chart.ChartType = 4; $chart.SetSourceData($sheet.Range('H1:H2'))
                $line = $chart.SeriesCollection(1).Format.Line
                $line.Visible = -1; $line.DashStyle = 1; $line.Weight = 0.75; $line.ForeColor.RGB = 255
                @{color=$line.ForeColor.RGB; weight=$line.Weight; dash=$line.DashStyle}
            }
            $entry.setup.chart_sheet = GetOrError {
                $chartSheet = $book.Charts.Add(); $chartSheet.Name = 'ChartTab'
                $chartSheet.ChartType = 51; $chartSheet.SetSourceData($sheet.Range('H1:H2'))
                $chartSheet.HasTitle = $true; $chartSheet.ChartTitle.Text = 'Controlled chart sheet'
                @{name=[string]$chartSheet.Name; type=$chartSheet.ChartType;
                  series=[string]$chartSheet.SeriesCollection(1).Formula; title=[string]$chartSheet.ChartTitle.Text}
            }
            $entry.setup.chart_sheet_print = GetOrError {
                $chartSheet.PageSetup.CenterHeader = 'Controlled & header'
                $chartSheet.PageSetup.RightFooter = 'Page &P'
                $chartSheet.PageSetup.LeftMargin = 54; $chartSheet.PageSetup.RightMargin = 36
                $chartSheet.PageSetup.TopMargin = 72; $chartSheet.PageSetup.BottomMargin = 45
                $chartSheet.PageSetup.HeaderMargin = 18; $chartSheet.PageSetup.FooterMargin = 27
                @{header=[string]$chartSheet.PageSetup.CenterHeader; footer=[string]$chartSheet.PageSetup.RightFooter;
                  left=$chartSheet.PageSetup.LeftMargin; right=$chartSheet.PageSetup.RightMargin;
                  top=$chartSheet.PageSetup.TopMargin; bottom=$chartSheet.PageSetup.BottomMargin;
                  header_margin=$chartSheet.PageSetup.HeaderMargin; footer_margin=$chartSheet.PageSetup.FooterMargin}
            }
            $entry.setup.chart_sheet_page_setup = GetOrError {
                $chartSheet.PageSetup.Orientation = 2; $chartSheet.PageSetup.PaperSize = 9
                $chartSheet.PageSetup.BlackAndWhite = $true
                @{orientation=$chartSheet.PageSetup.Orientation; paper=$chartSheet.PageSetup.PaperSize;
                  black_and_white=$chartSheet.PageSetup.BlackAndWhite}
            }
            # All three chart-print sizes in one workbook provide an Excel-authored comparison.
            $entry.setup.chart_sheet_sizes = @(
                foreach ($size in @(1, 2, 3)) {
                    GetOrError {
                        $sized = $book.Charts.Add(); $sized.Name = "ChartSize$size"
                        $sized.ChartType = 51; $sized.SetSourceData($sheet.Range('H1:H2'))
                        $sized.PageSetup.ChartSize = $size
                        @{name=[string]$sized.Name; requested=$size; actual=$sized.PageSetup.ChartSize}
                    }
                }
            )
            $entry.setup.page = GetOrError { $sheet.PageSetup.Orientation = 2; $sheet.PageSetup.PrintArea = '$A$1:$D$5'; $sheet.PageSetup.Orientation }
            $entry.setup.header_picture = GetOrError {
                $picture = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../crates/calc-doc/tests/fixtures/drawing/pictures/card.png'))
                $sheet.PageSetup.LeftHeaderPicture.Filename = $picture
                $sheet.PageSetup.LeftHeader = '&G'
                @{header=[string]$sheet.PageSetup.LeftHeader; picture=[string]$sheet.PageSetup.LeftHeaderPicture.Filename}
            }
            $entry.setup.view = GetOrError {
                $sheet.Activate(); $window = $book.Windows.Item(1)
                $window.DisplayOutline = $false; $window.GridlineColorIndex = 5
                $window.View = 2; $window.Zoom = 80
                @{outline=$window.DisplayOutline; grid_color_index=$window.GridlineColorIndex; view=$window.View; zoom=$window.Zoom}
            }
            $entry.setup.phonetic = GetOrError {
                $p = $sheet.Range('F1'); $p.Value2 = [string]::Concat([char]0x6771, [char]0x4EAC)
                $reading = [string]::Concat([char]0x3068, [char]0x3046, [char]0x304D, [char]0x3087, [char]0x3046)
                $null = $p.Phonetics.Add(1, 2, $reading); $p.Phonetics.Visible = $true
                @{count=$p.Phonetics.Count; visible=$p.Phonetics.Visible}
            }
            $entry.setup.phonetic_long = GetOrError {
                $p = $sheet.Range('F2'); $p.Value2 = 'AB'
                $null = $p.Phonetics.Add(1, 2, ('a' * 5000)); $p.Phonetics.Visible = $true
                @{count=$p.Phonetics.Count; visible=$p.Phonetics.Visible}
            }
            $entry.setup.sort = GetOrError {
                $sortSheet = $book.Worksheets.Add(); $sortSheet.Name = 'SortData'
                $sortSheet.Range('A1').Value2 = 'Key'; $sortSheet.Range('B1').Value2 = 'Value'
                $sortSheet.Range('A2').Value2 = 2; $sortSheet.Range('B2').Value2 = 'second'
                $sortSheet.Range('A3').Value2 = 1; $sortSheet.Range('B3').Value2 = 'first'
                $missing = [Type]::Missing
                $null = $sortSheet.Range('A1:B3').Sort($sortSheet.Range('A1'), 1, $missing, $missing, 1, $missing, 1, 1)
                [string]$sortSheet.Range('A2').Value2
            }
            $entry.setup.chart_sheet_selection = GetOrError {
                $chartSheet.Activate(); $book.Windows.Item(1).Zoom = 75
                @{active=[string]$book.ActiveSheet.Name; zoom=$book.Windows.Item(1).Zoom}
            }
            $book.SaveAs($path, $fixture.legacy)
            $book.Close($false); $book=$null
            $entry.source_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($path))
            $entry.sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
            $book = $app.Workbooks.Open($path, 0, $false)
            $entry.before = Snapshot $book
            $book.SaveAs($saved, $fixture.modern)
            $entry.after = Snapshot $book
            $book.Close($false); $book=$null
            $entry.package_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($saved))
        } catch { $entry.error = $_.Exception.ToString() }
        finally { if ($book) { try {$book.Close($false)} catch {}; $book=$null } }
        $generated.Add($entry)
    }
    # These inputs are constructed by the test, not taken from the real-file corpus.
    # xlRepairFile (1) asks Excel to repair each malformed fixture explicitly.
    foreach ($name in $cases.repair_cases) {
        $name = [string]$name
        if ([IO.Path]::GetFileName($name) -ne $name) { throw "Input must be a fixture basename: $name" }
        $path = Join-Path $inputDir $name
        $entry = [ordered]@{file=$name; sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant(); error=$null}
        try {
            $missing = [Type]::Missing
            $book = $app.Workbooks.Open($path, 0, $true, $missing, $missing, $missing, $missing,
                $missing, $missing, $missing, $missing, $missing, $missing, $missing, 1)
            $entry.opened = Snapshot $book
            $saved = Join-Path $outputDir ([IO.Path]::GetFileNameWithoutExtension($name) + '-repaired.xlsx')
            $book.SaveAs($saved, 51)
            $entry.saved = Snapshot $book
            $book.Close($false); $book=$null
            $entry.package_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($saved))
        } catch { $entry.error = $_.Exception.ToString() }
        finally { if ($book) { try {$book.Close($false)} catch {}; $book=$null } }
        $repairs.Add($entry)
    }
    if ($cases.create_pivot) {
        $pivot = [ordered]@{error=$null}
        try {
            $book = $app.Workbooks.Add(-4167)
            $data = $book.Worksheets.Item(1); $data.Name = 'Data'
            $out = $book.Worksheets.Add([Type]::Missing, $data); $out.Name = 'Pivot'
            # Repeated source values exercise shared cache items and their record indexes.
            $values = [object[,]]::new(201,3)
            $values[0,0]='Region'; $values[0,1]='Amount'; $values[0,2]='Time'
            for ($i = 1; $i -le 200; $i++) {
                $values[$i,0] = if ($i % 2) {'North'} else {'South'}
                $values[$i,1] = [double](2 + ($i % 3))
                $values[$i,2] = [double](($i % 4) / 4.0)
            }
            $data.Range('A1:C201').Value2 = $values
            $data.Range('C2:C201').NumberFormat = 'h:mm'
            $cache = $book.PivotCaches().Create(1, 'Data!R1C1:R201C3', 1)
            $table = $cache.CreatePivotTable($out.Range('A3'), 'LegacyPivot', $true, 1)
            $table.PivotFields('Region').Orientation = 1
            $null = $table.AddDataField($table.PivotFields('Amount'), 'Sum of Amount', -4157)
            # A second view over the same cache exercises cache sharing, column placement and
            # a different aggregation. Preserve the first pivot if this optional setup fails.
            $pivot.second_error = $null
            try {
                $second = $cache.CreatePivotTable($out.Range('A12'), 'LegacyPivotColumns', $true, 1)
                $second.PivotFields('Region').Orientation = 2
                $null = $second.AddDataField($second.PivotFields('Amount'), 'Average Amount', -4106)
            } catch { $pivot.second_error = $_.Exception.ToString() }
            $pivot.named_error = $null
            try {
                $null = $book.Names.Add('cx_pivot_source', '=Data!$A$1:$C$201')
                $namedCache = $book.PivotCaches().Create(1, 'cx_pivot_source', 1)
                $pivot.named_refresh_setup = GetOrError { $namedCache.RefreshOnFileOpen = $true; $namedCache.RefreshOnFileOpen }
                $named = $namedCache.CreatePivotTable($out.Range('A22'), 'LegacyPivotNamed', $true, 1)
                $named.PivotFields('Region').Orientation = 1
                $null = $named.AddDataField($named.PivotFields('Amount'), 'Named Sum', -4157)
                $pivot.named_source = [string]$namedCache.SourceData
            } catch { $pivot.named_error = $_.Exception.ToString() }
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
@{fatal=$fatal; excel_version=[string]$app.Version; excel_build=[string]$app.Build; files=$rows.ToArray(); generated=$generated.ToArray(); repairs=$repairs.ToArray(); pivot=$pivot} |
    ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $Result -Encoding utf8NoBOM
