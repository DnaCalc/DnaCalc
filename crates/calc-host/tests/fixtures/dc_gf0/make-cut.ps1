# Authors cut-base.xlsx for dc-gf0 (cut and paste moving cells, defined names and other sheets)
# in Excel, then makes each case's cut and paste through Excel's own Range.Cut(Destination) on a
# fresh copy of it, saves the result as cut-<case>.xlsx and records in cut-excel.json, for each
# case, every sheet's formulas and constants (Range.Formula) over A1:J12 and every defined name's
# formula (RefersTo), as Excel holds them after the move. The calc-host case
# (tests/round_trip.rs, dc_gf0__) makes the same moves and compares.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-cut.ps1 -Dir <fixtures/dc_gf0>
#
# It stops only the Excel instance it started.
param([Parameter(Mandatory = $true)][string]$Dir)
$ErrorActionPreference = 'Stop'
Add-Type -Namespace GF0 -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'
$Dir = (Resolve-Path $Dir).Path
$base = Join-Path $Dir 'cut-base.xlsx'
$json = Join-Path $Dir 'cut-excel.json'

# Each case: name, source sheet, source range, target sheet, target top-left cell.
$cases = @(
  @('same-sheet', 'Data', 'A1:B3', 'Data', 'E5'),
  @('to-target-sheet', 'Data', 'A1:B3', 'Target Sheet', 'C2'),
  @('to-other', 'Data', 'A1:A3', 'Other', 'E1'),
  @('middle-to-target', 'Data', 'A2:A3', 'Target Sheet', 'F1')
)

function Read-Book($wb) {
  $sheets = [ordered]@{}
  foreach ($ws in $wb.Worksheets) {
    $cells = [ordered]@{}
    foreach ($cell in $ws.Range('A1:J14').Cells) {
      $f = [string]$cell.Formula
      if ($f -ne '') { $cells[$cell.Address($false, $false)] = $f }
    }
    $sheets[$ws.Name] = $cells
  }
  $names = [ordered]@{}
  foreach ($n in $wb.Names) { $names[$n.Name] = [string]$n.RefersTo }
  return [ordered]@{ sheets = $sheets; names = $names }
}

$excel = New-Object -ComObject Excel.Application
[uint32]$xpid = 0
[void][GF0.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$xpid)
try {
  $excel.Visible = $false; $excel.DisplayAlerts = $false
  $wb = $excel.Workbooks.Add()
  while ($wb.Worksheets.Count -lt 3) { [void]$wb.Worksheets.Add([Type]::Missing, $wb.Worksheets.Item($wb.Worksheets.Count)) }
  $data = $wb.Worksheets.Item(1); $data.Name = 'Data'
  $other = $wb.Worksheets.Item(2); $other.Name = 'Other'
  $target = $wb.Worksheets.Item(3); $target.Name = 'Target Sheet'

  # Data: A1:A3 values, B1:B3 formulas over them, over D1 (not moved) and over Other; C and D
  # outside every move.
  $data.Range('A1').Value2 = [double]1
  $data.Range('A2').Value2 = [double]2
  $data.Range('A3').Value2 = [double]3
  $data.Range('D1').Value2 = [double]7
  $data.Range('B1').Formula = '=A1*10'
  $data.Range('B2').Formula = '=SUM(A1:A3)'
  $data.Range('B3').Formula = '=A1+Other!A1+D1'
  $data.Range('C1').Formula = '=B1+1'
  $data.Range('C2').Formula = '=SUM(A1:A5)'
  $data.Range('C3').Formula = '=$A$2*A3'
  # Other: formulas reading Data's cells, by reference and by name.
  $other.Range('A1').Value2 = [double]100
  $other.Range('B1').Formula = '=Data!A1'
  $other.Range('B2').Formula = '=SUM(Data!A1:A3)'
  $other.Range('B3').Formula = '=Data!B1*2'
  $other.Range('B4').Formula = '=SUM(Data!A1:B3)'
  $other.Range('B5').Formula = '=Data!D1'
  # References to cells a move writes over (each case's target).
  $other.Range('B6').Formula = '=Data!E5'
  $other.Range('B7').Formula = '=''Target Sheet''!C2+''Target Sheet''!F1'
  $other.Range('B8').Formula = '=E2'
  $other.Range('B9').Formula = '=SUM(Data!A1:A4)'
  $other.Range('B10').Formula = '=SUM(Data!E5:E6)'
  $other.Range('B11').Formula = '=SUM(Data!E4:E6)'
  $other.Range('B12').Formula = '=SUM(Data!A1:C3)'
  $other.Range('B13').Formula = '=SUM(Data!A1:B4)'
  $target.Range('A1').Value2 = 'target'

  # Names: absolute cell and range, a range only partly moved, relative and mixed references, a
  # constant, and one scoped to Data.
  [void]$wb.Names.Add('InputCell', '=Data!$A$1')
  [void]$wb.Names.Add('InputRange', '=Data!$A$1:$A$3')
  [void]$wb.Names.Add('PartRange', '=Data!$A$1:$A$5')
  [void]$wb.Names.Add('MixedRow', '=Data!$A1')
  [void]$wb.Names.Add('Rate', '=0.05')
  [void]$data.Names.Add('LocalIn', '=Data!$A$2')
  $data.Range('C4').Formula = '=InputCell+SUM(InputRange)'
  $data.Range('D2').Formula = '=LocalIn*2'
  $data.Range('D3').Formula = '=SUM(PartRange)'
  $excel.Calculate()
  $wb.SaveAs($base, 51)
  $wb.Close($false)

  $book = $excel.Workbooks.Open($base)
  $out = [ordered]@{ excel = "$($excel.Version) build $($excel.Build)"; base = (Read-Book $book); cases = [ordered]@{} }
  $book.Close($false)
  foreach ($case in $cases) {
    $book = $excel.Workbooks.Open($base)
    $from = $book.Worksheets.Item($case[1]).Range($case[2])
    $to = $book.Worksheets.Item($case[3]).Range($case[4])
    [void]$from.Cut($to)
    $excel.Calculate()
    $out.cases[$case[0]] = [ordered]@{
      from = "$($case[1])!$($case[2])"; to = "$($case[3])!$($case[4])"; book = (Read-Book $book)
    }
    $book.SaveAs((Join-Path $Dir "cut-$($case[0]).xlsx"), 51)
    $book.Close($false)
  }
  $out | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $json
} finally {
  $excel.Quit()
  [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
  if ($xpid -ne 0) { Stop-Process -Id $xpid -Force -ErrorAction SilentlyContinue }
}
