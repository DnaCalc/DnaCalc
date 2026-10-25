# Authors formats.xlsx for dc-09v (DNA Calc's number formats and sizes) in Excel and records the
# text Excel shows for each formatted cell (Range.Text) and the sizes it reports, in
# excel-shown.json.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-formats.ps1 -Out <formats.xlsx> -Json <excel-shown.json>
#
# It stops only the Excel instance it started.
param([string]$Out, [string]$Json)
$ErrorActionPreference = 'Stop'
Add-Type -Namespace D9 -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'
$excel = New-Object -ComObject Excel.Application
[uint32]$xpid = 0
[void][D9.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$xpid)
try {
  $excel.Visible = $false; $excel.DisplayAlerts = $false
  $wb = $excel.Workbooks.Add()
  while ($wb.Worksheets.Count -gt 1) { $wb.Worksheets.Item($wb.Worksheets.Count).Delete() }
  $ws = $wb.Worksheets.Item(1); $ws.Name = 'Formats'
  $cells = @(
    @('A1', 1234.5, '#,##0.00'),
    @('A2', 0.25, '0%'),
    @('A3', 45322, 'yyyy-mm-dd'),
    @('A4', -5, '0.00;[Red](0.00)'),
    @('A5', 'text', '@'),
    @('A6', 1234567, '#,##0'),
    @('A7', '=A1*2', '0.0'),
    @('A8', 0.5, 'h:mm AM/PM'),
    @('A9', 3.14159, '0.000'),
    @('A10', 1500000, '0.00E+00'),
    @('A11', 12, '"Qty "0'),
    @('A12', 0.333333333333, 'General'),
    @('A15', -5, '0;[Color10]-0'),
    @('A16', 5, '[Color45]0'),
    @('A17', 5, '[Color53]0;-0'),
    @('E1', 123456.789, '0.00'),
    @('E2', 12, '0'),
    @('E3', 'long text that is wider than its column', '@')
  )
  foreach ($c in $cells) {
    $r = $ws.Range($c[0])
    $r.NumberFormat = $c[2]
    if ($c[1] -is [string] -and $c[1].StartsWith('=')) { $r.Formula = [string]$c[1] } elseif ($c[1] -is [string]) { $r.Value2 = [string]$c[1] } else { $r.Value2 = [double]$c[1] }
  }
  $ws.Columns.Item(1).ColumnWidth = 16
  $ws.Columns.Item(2).ColumnWidth = 25
  # Column E is 4 characters wide: a number wider than that shows as #, a text overflows.
  $ws.Columns.Item(5).ColumnWidth = 4
  $ws.Columns.Item(3).Hidden = $true
  $ws.Range('B1').Value2 = 'wide'
  $ws.Range('D1').Value2 = 'after hidden'
  $ws.Rows.Item(3).RowHeight = 30
  $ws.Rows.Item(13).Hidden = $true
  $ws.Range('A14').Value2 = 'below hidden'
  $excel.Calculate()
  $shown = [ordered]@{}
  foreach ($c in $cells) { $shown[$c[0]] = $ws.Range($c[0]).Text }
  # The font color each formatted cell shows (DisplayFormat, as RGB hex), for the palette colors.
  $colors = [ordered]@{}
  foreach ($a in @('A15', 'A16', 'A17', 'A4')) {
    $bgr = [int]$ws.Range($a).DisplayFormat.Font.Color
    $colors[$a] = '#{0:x2}{1:x2}{2:x2}' -f ($bgr -band 0xFF), (($bgr -shr 8) -band 0xFF), (($bgr -shr 16) -band 0xFF)
  }
  $geo = [ordered]@{
    excel = "$($excel.Version) build $($excel.Build)"
    shown = $shown
    colors = $colors
    widths = [ordered]@{ A = $ws.Columns.Item(1).ColumnWidth; B = $ws.Columns.Item(2).ColumnWidth; C_hidden = $ws.Columns.Item(3).Hidden }
    rows = [ordered]@{ h3 = $ws.Rows.Item(3).RowHeight; h1 = $ws.Rows.Item(1).RowHeight; r13_hidden = $ws.Rows.Item(13).Hidden }
  }
  $wb.SaveAs($Out, 51)
  $wb.Close($false)
  $geo | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 $Json
} finally {
  try { $excel.Quit() } catch {}
  [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
  if ($xpid -ne 0) { Get-Process -Id $xpid -ErrorAction SilentlyContinue | Stop-Process -Force }
}
