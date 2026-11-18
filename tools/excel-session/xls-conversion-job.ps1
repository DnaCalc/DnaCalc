# dc-4j0e.12: Excel conversion observations for controlled fixtures written by the calc-doc test.
# One owned Excel session; Result contains observations and each saved package as base64.
param($OwnedExcel, [string]$Cases, [string]$Result)
$ErrorActionPreference = 'Stop'
$app = $OwnedExcel
$casesPath = [IO.Path]::GetFullPath($Cases)
$manifest = Get-Content -LiteralPath $casesPath -Raw -Encoding utf8 | ConvertFrom-Json
if (-not $manifest.formulas -or -not $manifest.files -or -not $manifest.repair_cases) { throw 'Incomplete XLS session manifest' }
$inputDir = [IO.Path]::GetDirectoryName($casesPath)
$outputDir = Join-Path ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($Result))) ('xls-conversion-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($outputDir) | Out-Null
$rows = [Collections.Generic.List[object]]::new()
$generated = [Collections.Generic.List[object]]::new()
$repairs = [Collections.Generic.List[object]]::new()
$book = $null; $fatal = $null; $pivot = $null; $oldAlerts = $app.DisplayAlerts
function GetOrError([scriptblock]$f) { try { return & $f } catch { return @{unread=$_.Exception.Message} } }
function StrokeSnapshot($shape) {
    GetOrError {
        $line = $shape.Line
        $values = @{visible=$line.Visible; rgb=$line.ForeColor.RGB; color_type=$line.ForeColor.Type;
            weight=$line.Weight; transparency=$line.Transparency; dash=$line.DashStyle;
            start_arrow=$line.BeginArrowheadStyle; end_arrow=$line.EndArrowheadStyle}
        # A failed property read must not become numeric zero through a cast.
        foreach ($key in $values.Keys) { if ($null -eq $values[$key]) { throw "Missing stroke property: $key" } }
        $values.scheme = GetOrError {
            $value = $line.ForeColor.SchemeColor
            if ($null -eq $value) { throw 'Missing stroke scheme colour' }
            [int]$value
        }
        $values
    }
}
function CacheRefreshDates($workbook) {
    $workbook.PivotCaches() | ForEach-Object {
        GetOrError {
            $date = [datetime]$_.RefreshDate
            $serial = $date.ToOADate()
            @{index=[int]$_.Index; refresh_date_iso=$date.ToString('o');
                oa_date=$serial; oa_date_bits=[BitConverter]::DoubleToInt64Bits($serial).ToString('X16');
                workbook_date1904=[bool]$workbook.Date1904}
        }
    }
}
function Snapshot($workbook) {
    # Force formula calculation before reading values; an unfinished rebuild is a failed
    # observation, not permission to call the file's old cache a calculated answer.
    $app.CalculateFullRebuild()
    $calculationState = [int]$app.CalculationState
    if ($calculationState -ne 0) { throw "Excel calculation did not finish: state $calculationState" }
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
        $shapes = @($sheet.Shapes | ForEach-Object {
            $shape = $_
            @{name=[string]$shape.Name; type=$shape.Type;
              top_left=(GetOrError { [string]$shape.TopLeftCell.Address($false,$false) });
              bottom_right=(GetOrError { [string]$shape.BottomRightCell.Address($false,$false) });
              stroke=(StrokeSnapshot $shape)}
        })
        $sheets.Add(@{name=[string]$sheet.Name; visible=$sheet.Visible; cells=$cells.ToArray(); links=$links; shapes=$shapes;
            conditional_formats=(GetOrError {$used.FormatConditions.Count}); validations=(GetOrError {$used.SpecialCells(-4174).CountLarge});
            charts=(GetOrError {$sheet.ChartObjects().Count})})
    }
    $names = @($workbook.Names | ForEach-Object { @{name=[string]$_.Name; refers_to=[string]$_.RefersTo; visible=$_.Visible} })
    return @{date1904=$workbook.Date1904; sheets=$sheets.ToArray(); names=$names; file_format=$workbook.FileFormat;
        calculation=@{operation='CalculateFullRebuild'; state=$calculationState}}
}
try {
    $app.DisplayAlerts = $false
    foreach ($name in $manifest.files) {
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
        @{name='excel-biff8-arial10.xls'; legacy=56; modern=51; ext='.xlsx'; font='Arial'; font_size=10},
        @{name='excel-biff5.xls'; legacy=39; modern=51; ext='.xlsx'},
        # Microsoft XlFileFormat: https://learn.microsoft.com/en-us/office/vba/api/excel.xlfileformat
        @{name='excel-biff2.xls'; legacy=16; modern=51; ext='.xlsx'; raw=$true},
        @{name='excel-biff3.xls'; legacy=29; modern=51; ext='.xlsx'; raw=$true},
        @{name='excel-biff4.xls'; legacy=33; modern=51; ext='.xlsx'; raw=$true},
        @{name='excel-addin.xla'; legacy=18; modern=55; ext='.xlam'}
    )) {
        $entry = [ordered]@{file=$fixture.name; error=$null}
        try {
            $path = Join-Path $outputDir $fixture.name
            $saved = Join-Path $outputDir ([IO.Path]::GetFileNameWithoutExtension($fixture.name) + $fixture.ext)
            $book = $app.Workbooks.Add(-4167)
            $sheet = $book.Worksheets.Item(1); $sheet.Name = 'Data'
            if ($fixture.ContainsKey('font')) {
                $book.Styles.Item('Normal').Font.Name = $fixture.font
                $book.Styles.Item('Normal').Font.Size = $fixture.font_size
            }
            $sheet.Range('A1').Value2 = 42.5
            $sheet.Range('A2').Formula = '=A1+1'
            $sheet.Range('A1').NumberFormat = '0.00'
            if ($fixture.ContainsKey('raw')) {
                # A single controlled worksheet; ask Excel for old name/reference tokens.
                # SaveAs refusals are observations, never silently replaced by BIFF8 files.
                $entry.setup = [ordered]@{}
                $entry.setup.names = GetOrError {
                    $null = $book.Names.Add('RawCell', '=Data!$A$1')
                    $null = $book.Names.Add('RawArea', '=Data!$A$1:$A$2')
                    $sheet.Range('B1').Formula = '=SUM(RawArea)'
                    $sheet.Range('B2').Formula = '=RawCell+1'
                    @($book.Names | ForEach-Object { @{name=[string]$_.Name; refers_to=[string]$_.RefersTo} })
                }
                $entry.setup.grid = GetOrError {
                    $sheet.Activate(); $book.Windows.Item(1).GridlineColor = 5649426
                    $book.Windows.Item(1).GridlineColor
                }
            } else {
            $entry.tab_color_setup = GetOrError {
                $sheet.Tab.ColorIndex = 3
                @{color_index=$sheet.Tab.ColorIndex; color=$sheet.Tab.Color}
            }
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
            $entry.setup.comment_rich = GetOrError {
                $note = $sheet.Range('C3').Comment
                $note.Shape.TextFrame.Characters(1, 4).Font.Bold = $true
                $note.Shape.TextFrame.Characters(5, 4).Font.Italic = $true
                @{bold=$note.Shape.TextFrame.Characters(1, 4).Font.Bold;
                  italic=$note.Shape.TextFrame.Characters(5, 4).Font.Italic}
            }
            $entry.setup.comment_geometry = GetOrError {
                $shape = $sheet.Range('C3').Comment.Shape
                $shape.Left = 135; $shape.Top = 80; $shape.Width = 144; $shape.Height = 75
                @{left=$shape.Left; top=$shape.Top; width=$shape.Width; height=$shape.Height}
            }
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
            $entry.setup.shape_flowchart_process = GetOrError {
                $s = $sheet.Shapes.AddShape(61, 180, 100, 60, 30)
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_triangle = GetOrError {
                $s = $sheet.Shapes.AddShape(7, 250, 100, 60, 30)
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_flowchart_decision = GetOrError {
                $s = $sheet.Shapes.AddShape(63, 320, 100, 60, 30)
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_flowchart_connector = GetOrError {
                $s = $sheet.Shapes.AddShape(73, 390, 100, 60, 30)
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_right_triangle = GetOrError {
                $s = $sheet.Shapes.AddShape(8, 460, 100, 60, 30)
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_flowchart_extract = GetOrError {
                $s = $sheet.Shapes.AddShape(81, 530, 100, 60, 30)
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_flowchart_alternate_process = GetOrError {
                $s = $sheet.Shapes.AddShape(62, 600, 100, 60, 30)
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_down_arrow_wide = GetOrError {
                $s = $sheet.Shapes.AddShape(36, 670, 100, 90, 30)
                $s.TextFrame.Characters().Text = 'wide arrow'
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_down_arrow_tall = GetOrError {
                $s = $sheet.Shapes.AddShape(36, 770, 100, 30, 90)
                $s.TextFrame.Characters().Text = 'tall arrow'
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_rgb_solid_fill = GetOrError {
                $s = $sheet.Shapes.AddShape(1, 1110, 100, 90, 30)
                $s.Name = 'cx_rgb_solid_fill'
                $s.Fill.Visible = -1
                $s.Fill.Solid()
                $s.Fill.ForeColor.RGB = 0x563412
                $s.Fill.Transparency = 0
                @{name=[string]$s.Name; rgb=$s.Fill.ForeColor.RGB; transparency=$s.Fill.Transparency}
            }
            $entry.setup.shape_octagon = GetOrError {
                $s = $sheet.Shapes.AddShape(6, 810, 100, 60, 30)
                $s.TextFrame.Characters().Text = 'octagon'
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_parallelogram = GetOrError {
                $s = $sheet.Shapes.AddShape(2, 880, 100, 60, 30)
                $s.TextFrame.Characters().Text = 'parallelogram'
                @{name=[string]$s.Name; auto_shape_type=$s.AutoShapeType}
            }
            $entry.setup.shape_scripts = GetOrError {
                $s = $sheet.Shapes.AddTextbox(1, 950, 100, 90, 30)
                $s.TextFrame.Characters().Text = 'H2O x2'
                $s.TextFrame.Characters(2, 1).Font.Subscript = $true
                $s.TextFrame.Characters(6, 1).Font.Superscript = $true
                @{name=[string]$s.Name; subscript=$s.TextFrame.Characters(2, 1).Font.Subscript;
                  superscript=$s.TextFrame.Characters(6, 1).Font.Superscript}
            }
            $entry.setup.flipped_shapes = @(
                foreach ($direction in @(0, 1)) {
                    GetOrError {
                        $s = $sheet.Shapes.AddShape(1, 300, 170, 60, 30)
                        $s.Flip($direction)
                        @{name=[string]$s.Name; direction=$direction; horizontal=$s.HorizontalFlip; vertical=$s.VerticalFlip}
                    }
                }
            )
            $entry.setup.justified_text = GetOrError {
                $box = $sheet.Shapes.AddTextbox(1, 300, 100, 120, 55)
                $box.TextFrame.Characters().Text = 'A controlled justified text example with several words.'
                $box.TextFrame.HorizontalAlignment = -4130
                $box.TextFrame.MarginLeft = 7; $box.TextFrame.MarginTop = 8
                $box.TextFrame.MarginRight = 9; $box.TextFrame.MarginBottom = 10
                @{name=[string]$box.Name; horizontal_alignment=$box.TextFrame.HorizontalAlignment;
                  left=$box.TextFrame.MarginLeft; top=$box.TextFrame.MarginTop;
                  right=$box.TextFrame.MarginRight; bottom=$box.TextFrame.MarginBottom}
            }
            $entry.setup.rotated_text = @(
                foreach ($angle in @(-90, 90)) {
                    GetOrError {
                        $box = $sheet.Shapes.AddTextbox(1, 430, 100, 40, 100)
                        $box.TextFrame.Characters().Text = 'Rotated text'
                        $box.TextFrame.Orientation = $angle
                        @{name=[string]$box.Name; requested=$angle; orientation=$box.TextFrame.Orientation}
                    }
                }
            )
            $entry.setup.text_wrap = @(
                foreach ($wrap in @(0, -1)) {
                    GetOrError {
                        $box = $sheet.Shapes.AddTextbox(1, 430, 230, 80, 45)
                        $box.TextFrame.Characters().Text = 'Text long enough to exceed the box width'
                        $box.TextFrame2.WordWrap = $wrap
                        @{name=[string]$box.Name; requested=$wrap; word_wrap=$box.TextFrame2.WordWrap}
                    }
                }
            )
            $entry.setup.checkbox = GetOrError {
                $box = $sheet.CheckBoxes().Add(180, 145, 90, 24)
                $box.Caption = 'Controlled checkbox'; $box.Value = 1; $box.LinkedCell = '$M$2'
                @{name=[string]$box.Name; value=$box.Value; linked_cell=[string]$box.LinkedCell}
            }
            $entry.setup.shape_no_fill = GetOrError {
                $shape = $sheet.Shapes.AddShape(1, 100, 145, 60, 30)
                $shape.Fill.Visible = 0
                @{name=[string]$shape.Name; visible=$shape.Fill.Visible}
            }
            $entry.setup.shape_no_line = GetOrError {
                $shape = $sheet.Shapes.AddShape(1, 170, 145, 60, 30)
                $shape.Line.Visible = 0
                @{name=[string]$shape.Name; visible=$shape.Line.Visible}
            }
            $entry.setup.shape_solid_outline = GetOrError {
                $shape = $sheet.Shapes.AddShape(1, 240, 145, 60, 30)
                $shape.Line.Visible = -1; $shape.Line.ForeColor.RGB = 0x66ff66
                $shape.Line.Weight = 0.25; $shape.Line.Transparency = 0
                $shape.Line.DashStyle = 1
                @{name=[string]$shape.Name; stroke=(StrokeSnapshot $shape)}
            }
            $entry.setup.default_line = GetOrError {
                # Leave Excel's colour untouched: the corpus's dominant scheme-64
                # source lines need this observation before their colour is modelled.
                $shape = $sheet.Shapes.AddLine(100, 675, 220, 690)
                @{name=[string]$shape.Name; stroke=(StrokeSnapshot $shape)}
            }
            $entry.setup.straight_lines = @(foreach ($lineCase in @(
                @{kind='line'; x0=100; y0=700; x1=220; y1=730; arrow=1},
                @{kind='line'; x0=100; y0=760; x1=220; y1=740; arrow=1},
                @{kind='line'; x0=220; y0=800; x1=100; y1=780; arrow=1},
                @{kind='line'; x0=100; y0=820; x1=220; y1=820; arrow=1},
                @{kind='connector'; x0=100; y0=850; x1=220; y1=875; arrow=2}
            )) {
                GetOrError {
                    $shape = if ($lineCase.kind -eq 'connector') {
                        $sheet.Shapes.AddConnector(1, $lineCase.x0, $lineCase.y0, $lineCase.x1, $lineCase.y1)
                    } else {
                        $sheet.Shapes.AddLine($lineCase.x0, $lineCase.y0, $lineCase.x1, $lineCase.y1)
                    }
                    $shape.Line.Visible = -1
                    $shape.Line.ForeColor.RGB = 0x563412
                    $shape.Line.Weight = 0.75
                    $shape.Line.DashStyle = 1
                    $shape.Line.EndArrowheadStyle = $lineCase.arrow
                    @{name=[string]$shape.Name; requested=$lineCase; type=[int]$shape.Type;
                        left=[double]$shape.Left; top=[double]$shape.Top; width=[double]$shape.Width; height=[double]$shape.Height;
                        rotation=[double]$shape.Rotation; flip_h=[int]$shape.HorizontalFlip; flip_v=[int]$shape.VerticalFlip;
                        color=[int]$shape.Line.ForeColor.RGB; weight=[double]$shape.Line.Weight;
                        dash=[int]$shape.Line.DashStyle; end_arrow=[int]$shape.Line.EndArrowheadStyle}
                }
            })
            $entry.setup.linked_picture = GetOrError {
                $path = Join-Path $inputDir 'linked-picture.png'
                $picture = $sheet.Shapes.AddPicture($path, -1, 0, 100, 355, 60, 30)
                @{name=[string]$picture.Name; linked=$true}
            }
            $entry.setup.shape_group = GetOrError {
                $first = $sheet.Shapes.AddShape(1, 480, 420, 50, 25)
                $second = $sheet.Shapes.AddShape(9, 540, 430, 40, 30)
                $names = @([string]$first.Name, [string]$second.Name)
                $group = $sheet.Shapes.Range($names).Group()
                @{name=[string]$group.Name; children=$group.GroupItems.Count}
            }
            $entry.setup.picture_group = GetOrError {
                $path = Join-Path $inputDir 'linked-picture.png'
                $first = $sheet.Shapes.AddPicture($path, 0, -1, 100, 800, 50, 25)
                $second = $sheet.Shapes.AddPicture($path, 0, -1, 160, 810, 40, 30)
                $names = @([string]$first.Name, [string]$second.Name)
                $group = $sheet.Shapes.Range($names).Group()
                @{name=[string]$group.Name; children=$group.GroupItems.Count;
                  left=$group.Left; top=$group.Top; width=$group.Width; height=$group.Height;
                  column_a_width=$sheet.Columns.Item(1).Width; row_1_height=$sheet.Rows.Item(1).Height;
                  normal_font=[string]$book.Styles.Item('Normal').Font.Name;
                  normal_size=$book.Styles.Item('Normal').Font.Size}
            }
            $entry.setup.long_text_box = GetOrError {
                $box = $sheet.Shapes.AddTextbox(1, 100, 300, 160, 90)
                $text = ('é漢' * 5000)
                $box.TextFrame.Characters().Text = $text
                $box.TextFrame.Characters().Font.Bold = $true
                $box.TextFrame.Characters().Font.Italic = $true
                $box.TextFrame.Characters().Font.Name = 'Arial'
                $box.TextFrame.Characters().Font.Size = 12
                $box.TextFrame.Characters().Font.Color = 255
                $box.TextFrame.HorizontalAlignment = -4108
                $box.TextFrame.VerticalAlignment = -4108
                @{name=[string]$box.Name; characters=$box.TextFrame.Characters().Count;
                  bold=$box.TextFrame.Characters().Font.Bold; italic=$box.TextFrame.Characters().Font.Italic;
                  font_name=$box.TextFrame.Characters().Font.Name;
                  size=$box.TextFrame.Characters().Font.Size; color=$box.TextFrame.Characters().Font.Color;
                  horizontal_alignment=$box.TextFrame.HorizontalAlignment; vertical_alignment=$box.TextFrame.VerticalAlignment}
            }
            $entry.setup.line_shapes = @(
                foreach ($reverse in @($false, $true)) {
                    GetOrError {
                        $line = if ($reverse) {$sheet.Shapes.AddLine(100, 240, 180, 200)} else {$sheet.Shapes.AddLine(100, 200, 180, 240)}
                        $line.Line.ForeColor.RGB = 255; $line.Line.Weight = 0.75
                        @{name=[string]$line.Name; reverse=$reverse; color=$line.Line.ForeColor.RGB; weight=$line.Line.Weight}
                    }
                }
            )
            $entry.setup.chart_bar_geometry = GetOrError {
                $sheet.Range('H1').Value2 = 1; $sheet.Range('H2').Value2 = 2
                $c = $sheet.ChartObjects().Add(200, 100, 200, 150).Chart
                $c.ChartType = 51; $c.SetSourceData($sheet.Range('H1:H2'))
                $c.HasTitle = $true; $c.ChartTitle.Text = 'Controlled embedded title'
                $c.PlotVisibleOnly = $false; $c.DisplayBlanksAs = 3
                $group = $c.ChartGroups(1); $group.GapWidth = 70; $group.Overlap = -30; $group.VaryByCategories = $true
                $axis = $c.Axes(2); $axis.MinimumScale = 2; $axis.MaximumScale = 20
                $axis.MajorUnit = 5; $axis.MinorUnit = 1; $axis.ReversePlotOrder = $true
                @{gap=$group.GapWidth; overlap=$group.Overlap; varied=$group.VaryByCategories; title=[string]$c.ChartTitle.Text;
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
            $entry.setup.chart_print_setup = GetOrError {
                $chart = $sheet.ChartObjects().Item(1).Chart
                $chart.PageSetup.Orientation = 2; $chart.PageSetup.PaperSize = 9
                $chart.PageSetup.BlackAndWhite = $true
                @{orientation=$chart.PageSetup.Orientation; paper=$chart.PageSetup.PaperSize;
                  black_and_white=$chart.PageSetup.BlackAndWhite}
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
            $entry.setup.chart_default_font = GetOrError {
                $chart = $sheet.ChartObjects('cxCacheChart').Chart
                $chart.ChartArea.Font.Name = 'Calibri'
                $chart.ChartArea.Font.Bold = $false
                $chart.ChartArea.Font.Italic = $false
                @{font_name=[string]$chart.ChartArea.Font.Name; bold=$chart.ChartArea.Font.Bold;
                  italic=$chart.ChartArea.Font.Italic}
            }
            $entry.setup.chart_linked_title = GetOrError {
                $dataSheet.Range('D1').Value2 = 'Controlled linked title'
                $linkedChart = $sheet.ChartObjects('cxCacheChart').Chart
                $linkedChart.HasTitle = $true; $linkedChart.ChartTitle.Formula = '=ChartSource!$D$1'
                $linkedChart.ChartTitle.Font.Name = 'Arial'
                $linkedChart.ChartTitle.Font.Bold = $true
                $linkedChart.ChartTitle.Font.Italic = $true
                $linkedChart.ChartTitle.Font.Color = 255
                @{formula=[string]$linkedChart.ChartTitle.Formula; text=[string]$linkedChart.ChartTitle.Text;
                  font_name=[string]$linkedChart.ChartTitle.Font.Name; bold=$linkedChart.ChartTitle.Font.Bold;
                  italic=$linkedChart.ChartTitle.Font.Italic; color=$linkedChart.ChartTitle.Font.Color}
            }
            $entry.setup.chart_axis_font = GetOrError {
                $axis = $sheet.ChartObjects('cxCacheChart').Chart.Axes(1)
                $axis.TickLabels.Font.Name = 'Arial'
                $axis.TickLabels.Font.Bold = $true
                $axis.TickLabels.Font.Italic = $true
                @{font_name=[string]$axis.TickLabels.Font.Name; bold=$axis.TickLabels.Font.Bold;
                  italic=$axis.TickLabels.Font.Italic}
            }
            $entry.setup.chart_axis_titles = GetOrError {
                $chart = $sheet.ChartObjects('cxCacheChart').Chart
                $category = $chart.Axes(1); $category.HasTitle = $true
                $category.AxisTitle.Text = 'Region & area'
                $category.AxisTitle.Font.Name = 'Arial'
                $category.AxisTitle.Font.Bold = $true
                $category.AxisTitle.Font.Italic = $true
                $category.AxisTitle.Font.Color = 16711680
                $value = $chart.Axes(2); $value.HasTitle = $true
                $value.AxisTitle.Text = 'Revenue'
                $value.AxisTitle.Font.Color = 255
                @{category=[string]$category.AxisTitle.Text; value=[string]$value.AxisTitle.Text;
                  font_name=[string]$category.AxisTitle.Font.Name; bold=$category.AxisTitle.Font.Bold;
                  italic=$category.AxisTitle.Font.Italic; category_color=$category.AxisTitle.Font.Color; value_color=$value.AxisTitle.Font.Color}
            }
            $entry.setup.chart_hidden_axis_labels = GetOrError {
                $chart = $sheet.ChartObjects().Add(620, 100, 200, 150).Chart
                $chart.ChartType = 51; $chart.SetSourceData($dataSheet.Range('A1:B3'))
                $category = $chart.Axes(1); $category.TickLabelPosition = -4142
                $category.Format.Line.Visible = 0
                @{category_position=$category.TickLabelPosition; value_position=$chart.Axes(2).TickLabelPosition;
                  category_line_visible=$category.Format.Line.Visible}
            }
            $entry.setup.chart_axis_rotations = @(
                foreach ($angle in @(45, -45, 90, -90)) {
                    GetOrError {
                        $rotatedChart = $sheet.ChartObjects().Add(620, 270 + 160 * (($angle + 90) / 45), 200, 150).Chart
                        $rotatedChart.ChartType = 51; $rotatedChart.SetSourceData($dataSheet.Range('A1:B3'))
                        $rotatedChart.Axes(1).TickLabels.Orientation = $angle
                        @{requested=$angle; actual=$rotatedChart.Axes(1).TickLabels.Orientation}
                    }
                }
            )
            $entry.setup.chart_axis_number_format = GetOrError {
                $axis = $sheet.ChartObjects('cxCacheChart').Chart.Axes(2)
                $axis.TickLabels.NumberFormat = '0.00%'
                @{format=[string]$axis.TickLabels.NumberFormat; linked=$axis.TickLabels.NumberFormatLinked}
            }
            $entry.setup.chart_gridline_color = GetOrError {
                $axis = $sheet.ChartObjects('cxCacheChart').Chart.Axes(2)
                $axis.HasMajorGridlines = $true
                $line = $axis.MajorGridlines.Format.Line
                $line.Visible = -1; $line.ForeColor.RGB = 255; $line.Weight = 0.75
                @{rgb=$line.ForeColor.RGB; weight=$line.Weight; visible=$line.Visible}
            }
            $entry.setup.chart_axis_line_color = GetOrError {
                $chart = $sheet.ChartObjects().Add(830, 100, 200, 150).Chart
                $chart.ChartType = 51; $chart.SetSourceData($dataSheet.Range('A1:B3'))
                $line = $chart.Axes(1).Format.Line
                $line.Visible = -1; $line.ForeColor.RGB = 255; $line.Weight = 0.75
                @{rgb=$line.ForeColor.RGB; weight=$line.Weight; visible=$line.Visible}
            }
            $entry.setup.chart_value_axis_line_color = GetOrError {
                $chart = $sheet.ChartObjects().Add(830, 280, 200, 150).Chart
                $chart.ChartType = 51; $chart.SetSourceData($dataSheet.Range('A1:B3'))
                $line = $chart.Axes(2).Format.Line
                $line.Visible = -1; $line.ForeColor.RGB = 255; $line.Weight = 0.75
                @{rgb=$line.ForeColor.RGB; weight=$line.Weight; visible=$line.Visible}
            }
            $entry.setup.chart_legend_font = GetOrError {
                $chart = $sheet.ChartObjects('cxCacheChart').Chart
                $chart.HasLegend = $true
                $chart.Legend.Font.Name = 'Arial'
                $chart.Legend.Font.Bold = $true
                $chart.Legend.Font.Italic = $true
                $chart.Legend.Font.Color = 32768
                @{font_name=[string]$chart.Legend.Font.Name; bold=$chart.Legend.Font.Bold; italic=$chart.Legend.Font.Italic; color=$chart.Legend.Font.Color}
            }
            $entry.setup.chart_series_fill = GetOrError {
                $series = $sheet.ChartObjects('cxCacheChart').Chart.SeriesCollection(1)
                $series.Format.Fill.Visible = -1; $series.Format.Fill.Solid()
                $series.Format.Fill.ForeColor.RGB = 255
                @{fill_rgb=$series.Format.Fill.ForeColor.RGB}
            }
            $entry.setup.chart_value_labels = GetOrError {
                $series = $sheet.ChartObjects('cxCacheChart').Chart.SeriesCollection(1)
                $dataSheet.Range('B2:B3').NumberFormat = '0.00'
                $series.ApplyDataLabels(); $series.DataLabels().ShowValue = $true
                @{show_value=$series.DataLabels().ShowValue;
                  number_format_linked=$series.DataLabels().NumberFormatLinked;
                  first_text=[string]$series.DataLabels().Item(1).Text}
            }
            $entry.setup.chart_group_value_labels = GetOrError {
                $chart = $sheet.ChartObjects().Item(1).Chart
                $chart.ApplyDataLabels()
                $series = $chart.SeriesCollection(1)
                $series.DataLabels().ShowValue = $true
                @{show_value=$series.DataLabels().ShowValue}
            }
            $entry.setup.chart_series_line = GetOrError {
                $chart = $sheet.ChartObjects().Add(410, 270, 200, 150).Chart
                $chart.ChartType = 4; $chart.SetSourceData($sheet.Range('H1:H2'))
                $line = $chart.SeriesCollection(1).Format.Line
                $line.Visible = -1; $line.DashStyle = 1; $line.Weight = 0.75; $line.ForeColor.RGB = 255
                @{color=$line.ForeColor.RGB; weight=$line.Weight; dash=$line.DashStyle}
            }
            $entry.setup.chart_legend_positions = @(
                foreach ($case in @(@{index=1; position=-4160}, @{index=2; position=-4107},
                                    @{index=3; position=-4131}, @{index=4; position=2})) {
                    GetOrError {
                        $chart = $sheet.ChartObjects().Item($case.index).Chart
                        $chart.HasLegend = $true; $chart.Legend.Position = $case.position
                        @{index=$case.index; position=$chart.Legend.Position}
                    }
                }
            )
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
            $entry.setup.chart_sheet_protection = @(
                foreach ($mode in @(@{name='Both'; objects=$true; contents=$true},
                                    @{name='Objects'; objects=$true; contents=$false},
                                    @{name='Contents'; objects=$false; contents=$true})) {
                    GetOrError {
                        $protected = $book.Charts.Add(); $protected.Name = 'ChartProtect' + $mode.name
                        $protected.ChartType = 51; $protected.SetSourceData($sheet.Range('H1:H2'))
                        $protected.Protect([Type]::Missing, $mode.objects, $mode.contents)
                        @{name=[string]$protected.Name; contents=$protected.ProtectContents;
                          objects=$protected.ProtectDrawingObjects}
                    }
                }
            )
            $entry.setup.page = GetOrError { $sheet.PageSetup.Orientation = 2; $sheet.PageSetup.PrintArea = '$A$1:$D$5'; $sheet.PageSetup.Orientation }
            $entry.setup.print_comments = GetOrError {
                $sheet.PageSetup.PrintComments = 16
                @{mode=$sheet.PageSetup.PrintComments}
            }
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
    foreach ($name in $manifest.repair_cases) {
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
    if ($manifest.create_pivot) {
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
            $pivot.cache_refresh_users = @($book.PivotCaches() | ForEach-Object {
                GetOrError { @{index=[int]$_.Index; refresh_name=[string]$_.RefreshName} }
            })
            $pivot.cache_refresh_dates = @(CacheRefreshDates $book)
            $book.SaveAs($xlsx, 51)
            $book.Close($false); $book = $null
            $pivot.xlsx_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($xlsx))
            # A controlled encrypted twin probes cache-stream encryption. Never use a corpus file.
            $encrypted = [ordered]@{file='excel-pivot-encrypted.xls'; error=$null}
            try {
                $encryptedXls = Join-Path $outputDir 'pivot-encrypted.xls'
                $encryptedXlsx = Join-Path $outputDir 'pivot-encrypted.xlsx'
                $book = $app.Workbooks.Open($xls, 0, $false)
                $book.SaveAs($encryptedXls, 56, 'VelvetSweatshop')
                $book.Close($false); $book = $null
                $encrypted.source_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($encryptedXls))
                $encrypted.sha256 = (Get-FileHash -LiteralPath $encryptedXls -Algorithm SHA256).Hash.ToLowerInvariant()
                $book = $app.Workbooks.Open($encryptedXls, 0, $false, [Type]::Missing, 'VelvetSweatshop')
                $encrypted.before = Snapshot $book
                $book.SaveAs($encryptedXlsx, 51, '')
                $book.Close($false); $book = $null
                $encrypted.package_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($encryptedXlsx))
            } catch { $encrypted.error = $_.Exception.ToString() }
            finally { if ($book) { try {$book.Close($false)} catch {}; $book=$null } }
            $generated.Add($encrypted)
            # Refresh a controlled 1904-system twin to measure the cache date epoch.
            $date1904 = [ordered]@{file='excel-pivot-1904.xls'; error=$null}
            try {
                $legacy1904 = Join-Path $outputDir 'pivot-1904.xls'
                $modern1904 = Join-Path $outputDir 'pivot-1904.xlsx'
                $book = $app.Workbooks.Open($xlsx, 0, $false)
                $book.Date1904 = $true
                foreach ($sourceCache in $book.PivotCaches()) { $sourceCache.Refresh() }
                $book.SaveAs($legacy1904, 56)
                $book.Close($false); $book = $null
                $date1904.source_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($legacy1904))
                $date1904.sha256 = (Get-FileHash -LiteralPath $legacy1904 -Algorithm SHA256).Hash.ToLowerInvariant()
                $book = $app.Workbooks.Open($legacy1904, 0, $false)
                $date1904.before = Snapshot $book
                $date1904.cache_refresh_dates = @(CacheRefreshDates $book)
                $book.SaveAs($modern1904, 51)
                $book.Close($false); $book = $null
                $date1904.package_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($modern1904))
            } catch { $date1904.error = $_.Exception.ToString() }
            finally { if ($book) { try {$book.Close($false)} catch {}; $book=$null } }
            $generated.Add($date1904)
        } catch { $pivot.error = $_.Exception.ToString() }
        finally { if ($book) { try {$book.Close($false)} catch {}; $book=$null } }
    }
} catch { $fatal = $_.Exception.ToString() }
finally { if ($book) { try {$book.Close($false)} catch {} }; $app.DisplayAlerts = $oldAlerts }
@{fatal=$fatal; excel_version=[string]$app.Version; excel_build=[string]$app.Build; files=$rows.ToArray(); generated=$generated.ToArray(); repairs=$repairs.ToArray(); pivot=$pivot} |
    ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $Result -Encoding utf8NoBOM
