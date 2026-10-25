# Authors structural-base.xlsx for dc-jhn (D2b: rows and columns inserted and deleted through
# calc-host) in Excel, then makes the same insertions and deletions the host's case makes, in
# order, through Excel's own Insert and Delete (CopyOrigin left as Excel's default, the format of
# the row above or the column to the left), and saves the result as structural-after.xlsx. It
# records in structural-excel.json, for each sheet's A1:J16, each cell's formula, value and number
# format as Excel shows them after the edits, each row's height and hidden flag, each column's
# width and hidden flag, the defined names and the table's range. The values of
# structural-after.xlsx are captured separately (`cargo xtask capture`) into
# structural-after.capture.json.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-structural.ps1 -Dir <fixtures/d2>
#
# It stops only the Excel instance it started.
param([Parameter(Mandatory = $true)][string]$Dir)
$ErrorActionPreference = 'Stop'
Add-Type -Namespace D2S -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'
$Dir = (Resolve-Path $Dir).Path
$base = Join-Path $Dir 'structural-base.xlsx'
$after = Join-Path $Dir 'structural-after.xlsx'
$json = Join-Path $Dir 'structural-excel.json'

# The edits, in order: sheet, rows or columns, insert or delete, first position, count. The
# calc-host case (tests/round_trip.rs, d2b_structural__) makes the same ones.
$steps = @(
  @('S', 'rows', 'insert', 4, 2),
  @('S', 'rows', 'delete', 9, 1),
  @('S', 'columns', 'insert', 2, 1),
  @('S', 'columns', 'delete', 5, 1),
  @('S', 'rows', 'delete', 2, 1),
  @('T', 'rows', 'insert', 3, 1),
  @('U', 'rows', 'insert', 1, 1),
  @('U', 'columns', 'insert', 1, 2)
)

$missing = [System.Type]::Missing
$excel = New-Object -ComObject Excel.Application
[uint32]$xpid = 0
[void][D2S.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$xpid)
try {
  $excel.Visible = $false; $excel.DisplayAlerts = $false
  $wb = $excel.Workbooks.Add()
  while ($wb.Worksheets.Count -lt 3) { [void]$wb.Worksheets.Add([Type]::Missing, $wb.Worksheets.Item($wb.Worksheets.Count)) }
  $s = $wb.Worksheets.Item(1); $s.Name = 'S'
  $t = $wb.Worksheets.Item(2); $t.Name = 'T'
  $u = $wb.Worksheets.Item(3); $u.Name = 'U'

  # S: values 1..10 in A1:A10 and formulas over them, in rows, columns, whole columns, a name
  # and another sheet.
  for ($i = 1; $i -le 10; $i++) { $s.Range("A$i").Value2 = [double]$i }
  [void]$wb.Names.Add('N', '=S!$A$4:$A$6')
  $s.Range('B1').Formula = '=SUM(A1:A10)'
  $s.Range('B2').Formula = '=A5*2'
  $s.Range('B3').Formula = '=A$3+$A4'
  $s.Range('B4').Formula = '=SUM(A:A)'
  $s.Range('B5').Formula = '=SUM(N)'
  $s.Range('B6').Formula = '=T!B3*10'
  $s.Range('B7').Formula = '=A2*100'
  $s.Range('B8').Formula = '=SUM(A2:A4)'
  $s.Range('B9').Formula = '=COUNT(A6:A12)'
  $s.Range('C1').Value2 = 'label'
  $s.Range('D1').Formula = '=B1+C2'
  $s.Range('D2').Formula = '=SUM(B1:B3)'
  $s.Range('E1').Formula = '=D1*2'
  $s.Range('F1').Formula = '=SUM(C1:E1)'
  $s.Range('A3').NumberFormat = '0.00'
  $s.Range('B3').NumberFormat = '0.0%'
  $s.Rows.Item(3).RowHeight = 30
  $s.Rows.Item(6).Hidden = $true
  $s.Columns.Item(3).ColumnWidth = 20
  $s.Columns.Item(4).ColumnWidth = 6

  # T: a table whose data rows a row is inserted into, and formulas reading S.
  $t.Range('A1').Value2 = 'Item'; $t.Range('B1').Value2 = 'Units'
  $t.Range('A2').Value2 = 'a'; $t.Range('B2').Value2 = [double]10
  $t.Range('A3').Value2 = 'b'; $t.Range('B3').Value2 = [double]20
  $t.Range('A4').Value2 = 'c'; $t.Range('B4').Value2 = [double]30
  $list = $t.ListObjects.Add(1, $t.Range('A1:B4'), [Type]::Missing, 1)
  $list.Name = 'Sales'
  $t.Range('D1').Formula = '=SUM(Sales[Units])'
  $t.Range('D2').Formula = '=SUM(S!A2:A8)'
  $t.Range('D3').Formula = '=S!A5'
  $t.Range('D4').Formula = '=SUM(B2:B4)'

  # U: a formatted first row and column, then a row and two columns inserted before them.
  $u.Range('A1').Value2 = [double]0.5
  $u.Range('A1').NumberFormat = '0.0%'
  $u.Range('A2').Formula = '=A1*2'
  $u.Range('B1').Formula = '=A1+A2'
  $u.Rows.Item(1).RowHeight = 25
  $u.Columns.Item(1).ColumnWidth = 15

  # What lies on the cells and moves with them (compare-structural-parts.py): merged cells, a
  # conditional format and data validations whose formulas name cells (one on another sheet), a
  # hyperlink, notes (one on a row a deletion removes), a threaded comment, a shape, a page break
  # and a filter.
  [void]$s.Range('C5:D6').Merge()
  [void]$s.Range('E10:F11').Merge()
  [void]$s.Range('G3:G12').Merge()
  $condition = $s.Range('B1:B9').FormatConditions.Add(2, $missing, '=B1>A$3')
  $condition.Interior.Color = 255
  $validation = $s.Range('C8:C12').Validation
  $validation.Add(3, 1, 1, '=T!$A$2:$A$4')
  $validation = $t.Range('E2:E5').Validation
  $validation.Add(1, 1, 1, '=S!$A$1', '=S!$A$10')
  [void]$s.Hyperlinks.Add($s.Range('C3'), '', 'T!A1')
  [void]$s.Range('A2').AddComment('two')
  [void]$s.Range('A8').AddComment('eight')
  [void]$s.Range('J1').AddComment('jay')
  [void]$u.Range('A1').AddCommentThreaded('thread')
  [void]$s.Shapes.AddShape(1, $s.Range('D3').Left, $s.Range('D3').Top, 80, 60)
  [void]$s.HPageBreaks.Add($s.Range('A8'))
  [void]$s.Range('A1:A10').AutoFilter()
  $excel.Calculate()
  $wb.SaveAs($base, 51)

  foreach ($step in $steps) {
    $sheet = $wb.Worksheets.Item($step[0])
    $first = [int]$step[3]; $last = $first + [int]$step[4] - 1
    if ($step[1] -eq 'rows') { $range = $sheet.Range("$($first):$($last)") }
    else { $range = $sheet.Range($sheet.Cells.Item(1, $first), $sheet.Cells.Item(1, $last)).EntireColumn }
    if ($step[2] -eq 'insert') { [void]$range.Insert() } else { [void]$range.Delete() }
  }
  $excel.Calculate()

  function Column-Letters([int]$n) { $s = ''; while ($n -gt 0) { $r = ($n - 1) % 26; $s = [char](65 + $r) + $s; $n = [math]::Floor(($n - 1) / 26) }; return $s }
  $sheets = [ordered]@{}
  foreach ($sheet in @($s, $t, $u)) {
    $cells = [ordered]@{}
    for ($r = 1; $r -le 16; $r++) {
      for ($c = 1; $c -le 10; $c++) {
        $cell = $sheet.Cells.Item($r, $c)
        $formula = [string]$cell.Formula
        $value = $cell.Value2
        $format = [string]$cell.NumberFormat
        if ($formula -eq '' -and $format -eq 'General') { continue }
        $shown = if ($value -is [int]) { "error $value" } elseif ($null -eq $value) { '' } else { [string]$value }
        $cells["$(Column-Letters $c)$r"] = [ordered]@{ formula = $formula; value = $shown; format = $format }
      }
    }
    $rows = [ordered]@{}
    for ($r = 1; $r -le 16; $r++) { $rows["$r"] = [ordered]@{ height = $sheet.Rows.Item($r).RowHeight; hidden = [bool]$sheet.Rows.Item($r).Hidden } }
    $columns = [ordered]@{}
    for ($c = 1; $c -le 10; $c++) { $columns[(Column-Letters $c)] = [ordered]@{ width = $sheet.Columns.Item($c).ColumnWidth; hidden = [bool]$sheet.Columns.Item($c).Hidden } }
    $sheets[$sheet.Name] = [ordered]@{ cells = $cells; rows = $rows; columns = $columns }
  }
  $names = [ordered]@{}
  foreach ($name in $wb.Names) { $names[$name.Name] = [string]$name.RefersTo }
  $record = [ordered]@{
    excel = "$($excel.Version) build $($excel.Build)"
    steps = @($steps | ForEach-Object { "$($_[0]) $($_[1]) $($_[2]) $($_[3]) $($_[4])" })
    table = [string]$t.ListObjects.Item(1).Range.Address($false, $false)
    names = $names
    sheets = $sheets
  }
  $wb.SaveAs($after, 51)
  $wb.Close($false)
  $record | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 $json
} finally {
  try { $excel.Quit() } catch {}
  [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
  [GC]::Collect(); [GC]::WaitForPendingFinalizers()
  if ($xpid -ne 0) { Get-Process -Id $xpid -ErrorAction SilentlyContinue | Stop-Process -Force }
}
