# One owned-Excel job (dc-ymjf.2.65): EUROCONVERT with the Euro Currency Tools add-in open, over the arguments of
# the function bank's _EUROTOOL.EUROCONVERT rows (Excel answered #NAME? there: the add-in was not open).
# Parameters (named, as submit.ps1 passes them):
#   Evidence: the CalcFunc-Evidence checkout (its readings/excel-20527/admit2-answers-_eurotool.euroconvert-*.json);
#   Result:   the WitnessSet JSON this writes (function EUROCONVERT).
# EUROTOOL.XLAM is opened read-only from Excel's library folder (Workbooks.Open, as the dc-fetch-f0tp measurement
# did) and closed at the end; no add-in is installed and no setting of Excel is changed. Each row's arguments are
# written as cells of a new workbook (numbers and logicals as Value2, text into cells formatted as text, errors as
# error formulas, blanks left empty), the call =EUROCONVERT(<cells>) goes into column K by Formula2, and after a
# full calculation the inputs are read back: a row whose inputs Excel holds differently is rejected, never kept.
param($OwnedExcel, [string]$Evidence, [string]$Result)
$ErrorActionPreference = 'Stop'
$codes = @{2000='Null';2007='Div0';2015='Value';2023='Ref';2029='Name';2036='Num';2042='NA';2043='GettingData';2045='Spill';2050='Calc'}
$literals = @{Null='#NULL!';Div0='#DIV/0!';Value='#VALUE!';Ref='#REF!';Name='#NAME?';Num='#NUM!';NA='#N/A'}
function Bits([double]$d) { '0x' + ('{0:x16}' -f [BitConverter]::DoubleToInt64Bits($d)) }
function Answer($v) {
    if ($null -eq $v) { return $null }
    if ($v -is [double]) { return Bits $v }
    if ($v -is [string]) { return 'text:' + $v }
    if ($v -is [bool]) { return 'logical:' + $(if ($v) { 'True' } else { 'False' }) }
    if ($v -is [int]) { $n = $v - (-2146826288) + 2000; if ($codes.ContainsKey($n)) { return 'error:' + $codes[$n] } }
    return $null
}
# The witness form of a read-back input cell (as bank-coverage.py writes arguments).
function Arg($v) {
    if ($null -eq $v) { return $null }
    if ($v -is [double]) { return Bits $v }
    if ($v -is [string] -or $v -is [bool]) { return $v }
    if ($v -is [int]) { $n = $v - (-2146826288) + 2000; if ($codes.ContainsKey($n)) { return @{error = $codes[$n]} } }
    return '?unread'
}
$rows = @()
foreach ($file in Get-ChildItem -LiteralPath (Join-Path $Evidence 'readings/excel-20527') -Filter 'admit2-answers-_eurotool.euroconvert-*.json' | Sort-Object Name) {
    $doc = Get-Content -LiteralPath $file.FullName -Raw -Encoding utf8 | ConvertFrom-Json
    foreach ($w in $doc.witnesses) { $rows += [pscustomobject]@{id = [string]$w.id; args = @($w.args)} }
}
$library = Join-Path ([string]$OwnedExcel.LibraryPath) 'EUROTOOL.XLAM'
$addin = $OwnedExcel.Workbooks.Open($library, 0, $true)
$book = $OwnedExcel.Workbooks.Add(-4167)
$out = [ordered]@{function = 'EUROCONVERT'; witnesses = @(); capture_provenance = $null; capture_summary = $null}
$rejected = @{}
try {
    $observed = @(foreach ($a in $OwnedExcel.AddIns2) { if ([bool]$a.IsOpen) { [IO.Path]::GetFileName([string]$a.FullName) } })
    $ws = $book.Worksheets.Item(1)
    $columns = 'A','B','C','D','E','F','G','H','I','J'
    $asked = @()
    $r = 0
    foreach ($row in $rows) {
        $unsupported = @($row.args | Where-Object { $_ -is [array] -or ($_ -is [pscustomobject] -and -not $_.PSObject.Properties['error']) })
        if ($unsupported.Count -gt 0 -or $row.args.Count -gt 10) { $rejected['array or range argument: not asked'] = 1 + [int]$rejected['array or range argument: not asked']; continue }
        $r++
        $refs = @()
        for ($i = 0; $i -lt $row.args.Count; $i++) {
            $a = $row.args[$i]
            $cell = $ws.Range("$($columns[$i])$r")
            if ($null -eq $a) { }
            elseif ($a -is [bool]) { $cell.Value2 = $a }
            elseif ($a -is [string] -and $a -match '^0x[0-9a-fA-F]{16}$') { $cell.Value2 = [BitConverter]::Int64BitsToDouble([Convert]::ToInt64($a.Substring(2), 16)) }
            elseif ($a -is [string]) { $cell.NumberFormat = '@'; $cell.Value2 = $a }
            else { $cell.Formula = '=' + $literals[[string]$a.error] }
            $refs += "$($columns[$i])$r"
        }
        $ws.Range("K$r").Formula2 = '=EUROCONVERT(' + ($refs -join ',') + ')'
        $asked += [pscustomobject]@{row = $r; id = $row.id; args = $row.args}
    }
    $OwnedExcel.CalculateFull()
    foreach ($q in $asked) {
        $back = @(for ($i = 0; $i -lt $q.args.Count; $i++) { Arg $ws.Range("$($columns[$i])$($q.row)").Value2 })
        $same = (ConvertTo-Json -InputObject @($q.args) -Depth 5 -Compress) -ceq (ConvertTo-Json -InputObject $back -Depth 5 -Compress)
        $answer = Answer $ws.Range("K$($q.row)").Value2
        if (-not $same) { $rejected['changed input'] = 1 + [int]$rejected['changed input']; continue }
        if ($null -eq $answer) { $rejected['unread answer'] = 1 + [int]$rejected['unread answer']; continue }
        $out.witnesses += [ordered]@{id = 'addin-' + $q.id; args = $back; expected_bits = $answer}
    }
    $out.capture_provenance = [ordered]@{
        environment = [ordered]@{excel_build = [string]$OwnedExcel.Build; excel_version = [string]$OwnedExcel.Version; platform = 'Windows'; excel_input_plumbing = 'cell_value2_bulk'}
        calculation_settings = [ordered]@{Date1904 = [bool]$book.Date1904; CompatibilityVersion = [int]$book.CompatibilityVersion; UseSystemSeparators = [bool]$OwnedExcel.UseSystemSeparators; DecimalSeparator = [string]$OwnedExcel.DecimalSeparator; ThousandsSeparator = [string]$OwnedExcel.ThousandsSeparator; user_culture = [Globalization.CultureInfo]::CurrentCulture.Name}
        addins_open = @('EUROTOOL.XLAM'); addins_open_observed = $observed
        service = 'tools/excel-session/euroconvert-addin-job.ps1'; timestamp = [DateTime]::UtcNow.ToString('o')}
    $out.capture_summary = [ordered]@{requested = $rows.Count; accepted = $out.witnesses.Count; rejected = $rejected}
} finally {
    $book.Close($false)
    $addin.Close($false)
}
[IO.File]::WriteAllText($Result, (ConvertTo-Json -InputObject $out -Depth 8) + "`n", [Text.UTF8Encoding]::new($false))
