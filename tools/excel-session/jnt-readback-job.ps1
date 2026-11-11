# One owned-Excel job for the jnt oracle workbook (dc-vdma): what Excel does with each formula as written.
# Parameters (named, as submit.ps1 passes them): Workbook (the oracle.xlsx as committed: it opens in Excel, the
# formulas Excel refuses at entry being stored as the cells' text), Request (a JSON file), Result (the JSON file
# this writes).
# Request: {"sheets": {"<sheet>": <last row>}, "entries": [{"sheet","row","formula"}]}
#  1. file read-back: the workbook is opened read-only, fully calculated, and columns A..G of every row up to the
#     last row of each listed sheet are read as typed tokens (n:<16 hex>, s:<utf16 hex>, b:TRUE|FALSE,
#     e:<literal>, - for empty), with the formula (Formula2) of column D;
#  2. entry: each listed formula is entered in its own cell D<row> in the legacy form (Range.Formula) and then
#     in the dynamic form (Range.Formula2); a refusal (COM exception) is recorded with its HRESULT and message,
#     and after a full calculation the accepted cell's value is read as a token with its Text.
# The workbook is never saved. No setting of Excel is changed and no dialog is answered.
param($OwnedExcel, [string]$Workbook, [string]$Request, [string]$Result)
$ErrorActionPreference = 'Stop'
$req = Get-Content -LiteralPath $Request -Raw -Encoding utf8 | ConvertFrom-Json
$errorLiteral = @{2000='#NULL!';2007='#DIV/0!';2015='#VALUE!';2023='#REF!';2029='#NAME?';2036='#NUM!';2042='#N/A';2043='#GETTING_DATA';2045='#SPILL!';2046='#CONNECT!';2047='#BLOCKED!';2048='#UNKNOWN!';2049='#FIELD!';2050='#CALC!'}
function Utf16Hex([string]$text) {
    $bytes = [Text.Encoding]::Unicode.GetBytes($text)
    $sb = [Text.StringBuilder]::new()
    for ($i = 0; $i -lt $bytes.Length; $i += 2) { [void]$sb.Append(('{0:x4}' -f ([int]$bytes[$i] + 256 * [int]$bytes[$i + 1]))) }
    $sb.ToString()
}
function Tok($v) {
    if ($null -eq $v) { return '-' }
    if ($v -is [double]) { return 'n:' + ('{0:x16}' -f [BitConverter]::DoubleToInt64Bits($v)) }
    if ($v -is [string]) { return 's:' + (Utf16Hex $v) }
    if ($v -is [bool]) { return 'b:' + $(if ($v) { 'TRUE' } else { 'FALSE' }) }
    if ($v -is [int]) {
        $n = $v - (-2146826288) + 2000
        if ($errorLiteral.ContainsKey($n)) { return 'e:' + $errorLiteral[$n] }
        return 'e:code' + $v
    }
    return 'unread:' + $v.GetType().Name
}
$out = [ordered]@{fatal = $null; version = [string]$OwnedExcel.Version; build = [string]$OwnedExcel.Build; sheets = [ordered]@{}; entries = @()}
$book = $OwnedExcel.Workbooks.Open($Workbook, 0, $true)
try {
    $OwnedExcel.CalculateFull()
    foreach ($prop in $req.sheets.PSObject.Properties) {
        $ws = $book.Worksheets.Item($prop.Name)
        $last = [int]$prop.Value
        $range = $ws.Range("A1:G$last")
        $values = $range.Value2
        $formulas = $ws.Range("D1:D$last").Formula2
        $rows = @()
        for ($r = 1; $r -le $last; $r++) {
            $tokens = @(for ($c = 1; $c -le 7; $c++) { Tok $values[$r, $c] })
            $rows += [ordered]@{r = $r; tokens = $tokens; formula2 = [string]$formulas[$r, 1]}
        }
        $out.sheets[$prop.Name] = $rows
    }
    $results = @{}
    foreach ($form in 'legacy', 'dynamic') {
        $placed = @()
        foreach ($e in $req.entries) {
            $ws = $book.Worksheets.Item([string]$e.sheet)
            $cell = $ws.Cells.Item([int]$e.row, 4)
            $rec = [ordered]@{sheet = [string]$e.sheet; row = [int]$e.row; formula = [string]$e.formula; form = $form; refused = $false}
            try { $cell.ClearContents() | Out-Null } catch { $rec.clear_error = $_.Exception.Message }
            try {
                if ($form -eq 'legacy') { $cell.Formula = [string]$e.formula } else { $cell.Formula2 = [string]$e.formula }
                $rec.has_formula = [bool]$cell.HasFormula
            } catch {
                $rec.refused = $true
                $rec.hresult = ('0x{0:X8}' -f $_.Exception.HResult)
                $rec.message = $_.Exception.Message
                $rec.has_formula = [bool]$cell.HasFormula
            }
            $placed += [pscustomobject]@{rec = $rec; cell = $cell}
        }
        $OwnedExcel.CalculateFull()
        foreach ($p in $placed) {
            if (-not $p.rec.refused) {
                $p.rec.token = Tok $p.cell.Value2
                $p.rec.text = [string]$p.cell.Text
                $p.rec.read_formula = [string]$p.cell.Formula2
                $p.rec.has_array = [bool]$p.cell.HasArray
            }
            $out.entries += $p.rec
        }
    }
} catch { $out.fatal = $_.Exception.ToString() }
finally {
    try { $book.Close($false) } catch { $out.close_error = $_.Exception.Message }
}
$out | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Result -Encoding utf8NoBOM
