# Authors charts.xlsx for dc-6yw (DNA Calc's read-only charts) in Excel and records in
# charts-excel.txt what Excel draws for each chart: its type, title, the cells under its top-left
# and bottom-right corners, and each series' name, values and X values
# (Chart.SeriesCollection(i).Values / .XValues); then the same after B2 is set to 40 and the
# workbook recalculates, and charts-excel.pdf, the sheet as Excel prints it.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-charts.ps1 -Dir <fixtures/dc_6yw>
#
# Lines of charts-excel.txt, tab separated:
#   chart  <n>  <ChartType>  <title>  <top-left cell>  <bottom-right cell>
#   series <n>  <i>  <name>  <values, ';' separated>  <X values, ';' separated>
#   after  <n>  <i>  <values after the edit>  <X values after it>
# It stops only the Excel instance it started.
param([Parameter(Mandatory = $true)][string]$Dir)
$ErrorActionPreference = 'Stop'
Add-Type -Namespace CH6 -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'
$Dir = (Resolve-Path $Dir).Path
$book = Join-Path $Dir 'charts.xlsx'
$txt = Join-Path $Dir 'charts-excel.txt'
$pdf = Join-Path $Dir 'charts-excel.pdf'

function Joined($values) {
  if ($null -eq $values) { return '' }
  (@($values) | ForEach-Object { if ($null -eq $_) { '' } else { [string]$_ } }) -join ';'
}

$excel = New-Object -ComObject Excel.Application
[uint32]$xpid = 0
[void][CH6.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$xpid)
try {
  $excel.Visible = $false; $excel.DisplayAlerts = $false
  $wb = $excel.Workbooks.Add()
  $ws = $wb.Worksheets.Item(1); $ws.Name = 'Data'
  $ws.Range('A1:D1').Value2 = [object[]]@('Month', 'North', 'South', 'Total')
  $months = @('Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun')
  $north = @(12, 18, 9, 22, 15, 27)
  $south = @(8, 11, 14, 10, 19, 13)
  for ($i = 0; $i -lt 6; $i++) {
    $r = $i + 2
    $ws.Range("A$r").Value2 = $months[$i]
    $ws.Range("B$r").Value2 = [double]$north[$i]
    $ws.Range("C$r").Value2 = [double]$south[$i]
    $ws.Range("D$r").Formula = "=B$r+C$r"
  }

  # ChartObjects.Add(Left, Top, Width, Height) in points; SetSourceData and ChartType.
  # Types: 51 clustered column, 58 stacked bar, 4 line, 1 area, 5 pie, -4169 XY scatter.
  $specs = @(
    @{ Type = 51; Source = 'A1:C7'; Title = 'Column'; Left = 300; Top = 10 },
    @{ Type = 58; Source = 'A1:C7'; Title = 'Bar'; Left = 600; Top = 10 },
    @{ Type = 4; Source = 'A1:A7,D1:D7'; Title = 'Line'; Left = 300; Top = 230 },
    @{ Type = 1; Source = 'A1:C7'; Title = 'Area'; Left = 600; Top = 230 },
    @{ Type = 5; Source = 'A1:B7'; Title = 'Pie'; Left = 300; Top = 450 },
    @{ Type = -4169; Source = 'B1:C7'; Title = 'Scatter'; Left = 600; Top = 450 }
  )
  foreach ($s in $specs) {
    $object = $ws.ChartObjects().Add($s.Left, $s.Top, 280, 200)
    $chart = $object.Chart
    $chart.ChartType = $s.Type
    $chart.SetSourceData($ws.Range($s.Source))
    $chart.HasTitle = $true
    $chart.ChartTitle.Text = $s.Title
  }
  $excel.Calculate()
  $wb.SaveAs($book, 51)

  $lines = New-Object System.Collections.Generic.List[string]
  $count = $ws.ChartObjects().Count
  for ($n = 1; $n -le $count; $n++) {
    $object = $ws.ChartObjects($n)
    $chart = $object.Chart
    $title = if ($chart.HasTitle) { $chart.ChartTitle.Text } else { '' }
    $lines.Add("chart`t$n`t$($chart.ChartType)`t$title`t$($object.TopLeftCell.Address($false, $false))`t$($object.BottomRightCell.Address($false, $false))")
    $series = $chart.SeriesCollection()
    for ($i = 1; $i -le $series.Count; $i++) {
      $one = $series.Item($i)
      $lines.Add("series`t$n`t$i`t$($one.Name)`t$(Joined $one.Values)`t$(Joined $one.XValues)")
    }
  }
  $ws.Range('B2').Value2 = [double]40
  $excel.Calculate()
  for ($n = 1; $n -le $count; $n++) {
    $series = $ws.ChartObjects($n).Chart.SeriesCollection()
    for ($i = 1; $i -le $series.Count; $i++) {
      $lines.Add("after`t$n`t$i`t$(Joined $series.Item($i).Values)`t$(Joined $series.Item($i).XValues)")
    }
  }
  [System.IO.File]::WriteAllText($txt, (($lines -join "`n") + "`n"))
  $ws.Range('B2').Value2 = [double]12
  $ws.PageSetup.Orientation = 2
  $ws.PageSetup.Zoom = $false
  $ws.PageSetup.FitToPagesWide = 1
  $ws.PageSetup.FitToPagesTall = 1
  $ws.ExportAsFixedFormat(0, $pdf)
  $wb.Close($false)
} finally {
  $excel.Quit()
  [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
  if ($xpid -ne 0) { Stop-Process -Id $xpid -Force -ErrorAction SilentlyContinue }
}
