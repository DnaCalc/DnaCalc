# Authors conditional-formats.xlsx for dc-6yw (DNA Calc shows conditional formats) in Excel and
# records in cf-excel.json what Excel shows for each cell of A1:N10 with its rules applied
# (Range.DisplayFormat: the fill colour, or none; the font colour; bold; italic) and the text
# it shows (Range.Text), and the same in cf-excel.txt, one cell per line, and exports the sheet's A1:N10 as Excel prints it (cf-excel.pdf), for the
# data bars and icon sets, which DisplayFormat does not report.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-cf.ps1 -Dir <fixtures/dc_6yw>
#
# It stops only the Excel instance it started.
param([Parameter(Mandatory = $true)][string]$Dir)
$ErrorActionPreference = 'Stop'
Add-Type -Namespace CF6 -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'
$Dir = (Resolve-Path $Dir).Path
$book = Join-Path $Dir 'conditional-formats.xlsx'
$json = Join-Path $Dir 'cf-excel.json'
$txt = Join-Path $Dir 'cf-excel.txt'
$pdf = Join-Path $Dir 'cf-excel.pdf'

function Rgb($r, $g, $b) { return [int]($r + 256 * $g + 65536 * $b) }

$excel = New-Object -ComObject Excel.Application
[uint32]$xpid = 0
[void][CF6.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$xpid)
try {
  $excel.Visible = $false; $excel.DisplayAlerts = $false
  $wb = $excel.Workbooks.Add()
  while ($wb.Worksheets.Count -gt 1) { $wb.Worksheets.Item($wb.Worksheets.Count).Delete() }
  $ws = $wb.Worksheets.Item(1); $ws.Name = 'CF'

  # Values: the same ten numbers in A:K (in another order in some columns), text in L, blanks in M.
  $numbers = @(4, 9, 1, 7, 3, 10, 6, 2, 8, 5)
  $words = @('banana', 'apple', 'cherry', 'mango', 'kiwi', 'orange', 'grape', 'plum', 'lemon', 'pear')
  for ($i = 0; $i -lt 10; $i++) {
    $r = $i + 1
    foreach ($col in 'A', 'B', 'C', 'E', 'F', 'G', 'H', 'I', 'J') { $ws.Range("$col$r").Value2 = [double]$numbers[$i] }
    $ws.Range("D$r").Formula = "=A$r*2"
    $ws.Range("K$r").Value2 = [double]($numbers[$i] % 4)
    $ws.Range("L$r").Value2 = $words[$i]
    if ($i % 3 -ne 0) { $ws.Range("M$r").Value2 = [double]$r }
    $ws.Range("N$r").Value2 = [double]$numbers[$i]
  }

  # A: cell value greater than 5: light red fill, dark red text (Excel's preset colours).
  $fc = $ws.Range('A1:A10').FormatConditions.Add(1, 5, '=5')
  $fc.Interior.Color = (Rgb 255 199 206); $fc.Font.Color = (Rgb 156 0 6)
  # B: between 3 and 6: green fill, bold.
  $fc = $ws.Range('B1:B10').FormatConditions.Add(1, 1, '=3', '=6')
  $fc.Interior.Color = (Rgb 198 239 206); $fc.Font.Bold = $true
  # C: a formula, every other row: yellow fill.
  $fc = $ws.Range('C1:C10').FormatConditions.Add(2, [Type]::Missing, '=MOD(ROW(),2)=0')
  $fc.Interior.Color = (Rgb 255 235 156)
  # D: a formula reading another column, relative row: italic blue text.
  $fc = $ws.Range('D1:D10').FormatConditions.Add(2, [Type]::Missing, '=$A1>6')
  $fc.Font.Italic = $true; $fc.Font.Color = (Rgb 0 0 255)
  # E: a three-colour scale (Excel's default: red, yellow, green).
  [void]$ws.Range('E1:E10').FormatConditions.AddColorScale(3)
  # F: a two-colour scale.
  [void]$ws.Range('F1:F10').FormatConditions.AddColorScale(2)
  # G: a data bar.
  [void]$ws.Range('G1:G10').FormatConditions.AddDatabar()
  # H: an icon set (three arrows).
  $fc = $ws.Range('H1:H10').FormatConditions.AddIconSetCondition()
  $fc.IconSet = $wb.IconSets.Item(1)
  # I: the top three: orange fill.
  $fc = $ws.Range('I1:I10').FormatConditions.AddTop10()
  $fc.TopBottom = 1; $fc.Rank = 3; $fc.Interior.Color = (Rgb 255 192 0)
  # J: above average: an accent theme colour, lightened.
  $fc = $ws.Range('J1:J10').FormatConditions.AddAboveAverage()
  $fc.AboveBelow = 0; $fc.Interior.ThemeColor = 5; $fc.Interior.TintAndShade = 0.6
  # K: duplicate values: purple text.
  $fc = $ws.Range('K1:K10').FormatConditions.AddUniqueValues()
  $fc.DupeUnique = 1; $fc.Font.Color = (Rgb 112 48 160)
  # L: text containing "an": bold, grey fill.
  $fc = $ws.Range('L1:L10').FormatConditions.Add(9, [Type]::Missing, [Type]::Missing, [Type]::Missing, 'an', 0)
  $fc.Font.Bold = $true; $fc.Interior.Color = (Rgb 217 217 217)
  # M: blanks: light blue fill.
  $fc = $ws.Range('M1:M10').FormatConditions.Add(10)
  $fc.Interior.Color = (Rgb 221 235 247)
  # N: two rules: the first (priority 1) above 7 stops; the second below 9 fills and formats as a
  # percentage.
  $fc = $ws.Range('N1:N10').FormatConditions.Add(1, 5, '=7')
  $fc.Interior.Color = (Rgb 255 0 0)
  $fc2 = $ws.Range('N1:N10').FormatConditions.Add(1, 6, '=9')
  $fc2.Interior.Color = (Rgb 0 176 80); $fc2.NumberFormat = '0.0%'
  $fc2.SetFirstPriority()
  $fc.SetFirstPriority(); $fc.StopIfTrue = $true

  $excel.Calculate()
  $wb.SaveAs($book, 51)

  $cells = [ordered]@{}
  foreach ($cell in $ws.Range('A1:N10').Cells) {
    $d = $cell.DisplayFormat
    $fill = if ($d.Interior.ColorIndex -eq -4142) { $null } else { '#{0:x6}' -f ((($d.Interior.Color -band 0xff) -shl 16) -bor ($d.Interior.Color -band 0xff00) -bor (($d.Interior.Color -shr 16) -band 0xff)) }
    $fc = [int]$d.Font.Color
    $cells[$cell.Address($false, $false)] = [ordered]@{
      fill = $fill
      font = ('#{0:x6}' -f ((($fc -band 0xff) -shl 16) -bor ($fc -band 0xff00) -bor (($fc -shr 16) -band 0xff)))
      bold = [bool]$d.Font.Bold
      italic = [bool]$d.Font.Italic
      text = [string]$cell.Text
    }
  }
  [ordered]@{ excel = "$($excel.Version) build $($excel.Build)"; sheet = 'CF'; cells = $cells } |
    ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 $json
  # The same, one cell per line (cell, fill or -, font colour, bold, italic, text), for the
  # calc-host case to read without a JSON parser.
  $lines = foreach ($key in $cells.Keys) {
    $c = $cells[$key]
    $fill = if ($c.fill) { $c.fill } else { '-' }
    "$key`t$fill`t$($c.font)`t$([int]$c.bold)`t$([int]$c.italic)`t$($c.text)"
  }
  [System.IO.File]::WriteAllText($txt, (($lines -join "`n") + "`n"))

  # The range as Excel draws it, for the data bars and the icons.
  # (As a PDF: a hidden Excel has no screen or clipboard to copy a picture from.)
  $ws.Range('A1:N10').ExportAsFixedFormat(0, $pdf)
  $wb.Close($false)
} finally {
  $excel.Quit()
  [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
  if ($xpid -ne 0) { Stop-Process -Id $xpid -Force -ErrorAction SilentlyContinue }
}
