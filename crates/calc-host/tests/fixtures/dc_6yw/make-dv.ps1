# Authors data-validation.xlsx for dc-6yw (DNA Calc's data validation) in Excel and records in
# dv-excel.txt, for candidate entries in each validated cell, whether Excel takes the cell's value
# as valid (Range.Validation.Value, read after the value is written through COM, which does not
# apply the validation), with the rule's error style, title and message and its input message;
# and the drop-down lists' items as Excel lists them.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-dv.ps1 -Dir <fixtures/dc_6yw>
#
# Each line of dv-excel.txt: cell, candidate (as typed), 1 when valid else 0. The date rule's
# candidates are serial numbers (45444 is 2024-06-01): DNA Calc's entry does not read a typed
# en-US date yet (calc-host's EntryRefusal::NumberShaped).
# It stops only the Excel instance it started.
param([Parameter(Mandatory = $true)][string]$Dir)
$ErrorActionPreference = 'Stop'
Add-Type -Namespace DV6 -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint pid);'
$Dir = (Resolve-Path $Dir).Path
$book = Join-Path $Dir 'data-validation.xlsx'
$txt = Join-Path $Dir 'dv-excel.txt'

$excel = New-Object -ComObject Excel.Application
[uint32]$xpid = 0
[void][DV6.Win]::GetWindowThreadProcessId([System.IntPtr]$excel.Hwnd, [ref]$xpid)
try {
  $excel.Visible = $false; $excel.DisplayAlerts = $false
  $wb = $excel.Workbooks.Add()
  while ($wb.Worksheets.Count -lt 2) { [void]$wb.Worksheets.Add([Type]::Missing, $wb.Worksheets.Item($wb.Worksheets.Count)) }
  $ws = $wb.Worksheets.Item(1); $ws.Name = 'DV'
  $lists = $wb.Worksheets.Item(2); $lists.Name = 'Lists'
  $ws.Range('K1').Value2 = 'North'; $ws.Range('K2').Value2 = 'South'; $ws.Range('K3').Value2 = 'East'
  $lists.Range('A1').Value2 = [double]10; $lists.Range('A2').Value2 = [double]20; $lists.Range('A3').Value2 = [double]30
  [void]$wb.Names.Add('Amounts', '=Lists!$A$1:$A$3')

  # Validation.Add(Type, AlertStyle, Operator, Formula1, Formula2)
  # Types: 1 whole, 2 decimal, 3 list, 4 date, 5 time, 6 text length, 7 custom.
  # Alert styles: 1 stop, 2 warning, 3 information. Operators: 1 between, 5 greater, 8 less or equal.
  $v = $ws.Range('A1:A5').Validation; $v.Add(3, 1, 1, 'Red,Green,Blue')
  $v.InputTitle = 'Colour'; $v.InputMessage = 'Pick a colour'; $v.ErrorTitle = 'Not a colour'; $v.ErrorMessage = 'Choose Red, Green or Blue'
  $v = $ws.Range('B1:B5').Validation; $v.Add(3, 1, 1, '=$K$1:$K$3')
  $v = $ws.Range('C1:C5').Validation; $v.Add(1, 1, 1, '1', '10')
  $v.ErrorTitle = 'Out of range'; $v.ErrorMessage = 'A whole number from 1 to 10'
  $v = $ws.Range('D1:D5').Validation; $v.Add(2, 2, 5, '0')
  $v.ErrorMessage = 'A positive number is expected'
  $v = $ws.Range('E1:E5').Validation; $v.Add(6, 3, 8, '5')
  $v.ErrorMessage = 'At most five characters'
  $v = $ws.Range('F1:F5').Validation; $v.Add(4, 1, 5, '=DATE(2024,1,1)')
  $v = $ws.Range('G1:G5').Validation; $v.Add(3, 1, 1, '=Amounts')
  $v = $ws.Range('H1:H5').Validation; $v.Add(1, 1, 1, '1', '10'); $v.IgnoreBlank = $false
  $wb.SaveAs($book, 51)

  $candidates = @(
    @('A1', 'Red'), @('A1', 'green'), @('A1', 'Purple'), @('A1', '1'),
    @('B1', 'North'), @('B1', 'north'), @('B1', 'West'),
    @('C1', '5'), @('C1', '0'), @('C1', '10'), @('C1', '2.5'), @('C1', 'x'),
    @('D1', '3'), @('D1', '-1'), @('D1', '0'),
    @('E1', 'abc'), @('E1', 'abcdef'), @('E1', '12345'), @('E1', '123456'),
    @('F1', '45444'), @('F1', '45291'), @('F1', '45292'), @('F1', '45293'),
    @('G1', '20'), @('G1', '25'),
    @('C2', ''), @('H1', '')
  )
  $lines = foreach ($c in $candidates) {
    $cell = $ws.Range($c[0])
    if ($c[1] -eq '') { [void]$cell.ClearContents() } else { $cell.Formula = $c[1] }
    $valid = [bool]$cell.Validation.Value
    "$($c[0])`t$($c[1])`t$([int]$valid)"
    [void]$cell.ClearContents()
  }
  [System.IO.File]::WriteAllText($txt, (($lines -join "`n") + "`n"))
  $wb.Close($false)
} finally {
  $excel.Quit()
  [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($excel)
  if ($xpid -ne 0) { Stop-Process -Id $xpid -Force -ErrorAction SilentlyContinue }
}
