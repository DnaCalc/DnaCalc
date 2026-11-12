# One owned-Excel job for OneCalc's number-format oracle (dc-0f1k, milestone 3): the text Excel
# displays for a value under a number format, by locale and date system.
# Parameters (named, as submit.ps1 passes them): Cases (the TSV this reads), Output (the TSV this
# writes) and Result (a JSON summary the host reads). A case row is: id, culture, decimal separator, thousands separator (each a literal
# character, or U+NNNN), LCID prefix for date names (hex, may be empty), date system (1900 or
# 1904), value kind (number, text or fn: the cell holds =TEXT(value, code)), value and format code. The result row repeats the id and
# adds the displayed text (Range.Text, columns wide enough that no value shows as #####), the
# invariant NumberFormat Excel read back from the localized code, and any error.
# The locale axis is what an Excel instance can be told: its decimal and thousands separators
# (Application.UseSystemSeparators = False and the two separator properties, restored at the end
# and by the host's baseline check) and, for date names, a [$-LCID] prefix on the format code.
# Format code letters stay English. No other setting is changed.
param($OwnedExcel, [string]$Cases, [string]$Output, [string]$Result, [string]$RegionalCulture = '')
$ErrorActionPreference = 'Stop'
function Literal([string]$text) {
    return [regex]::Replace($text, 'U\+([0-9A-Fa-f]{4})', { param($m) [string][char][Convert]::ToInt32($m.Groups[1].Value, 16) })
}
function Show([string]$text) {
    $out = New-Object System.Text.StringBuilder
    foreach ($ch in $text.ToCharArray()) {
        $code = [int]$ch
        if ($code -lt 32 -or $code -gt 126 -or $ch -eq '\') { [void]$out.AppendFormat('\u{0:X4}', $code) } else { [void]$out.Append($ch) }
    }
    return $out.ToString()
}
$app = $OwnedExcel
$before = [ordered]@{ system = $app.UseSystemSeparators; decimal = $app.DecimalSeparator; thousands = $app.ThousandsSeparator }
$rows = Get-Content -LiteralPath $Cases -Encoding utf8 | Where-Object { $_ -and -not $_.StartsWith('#') }
$book = $null
try {
if ($RegionalCulture) {
    # Revisit the rows whose previous comparison was ambiguous. Do not emulate the locale:
    # Excel must inherit the real Windows regional settings in a newly launched process.
    if ((Get-Culture).Name -ne $RegionalCulture) { throw "Windows culture is $((Get-Culture).Name), expected $RegionalCulture" }
    $rows = @($rows | Where-Object {
        $f = $_ -split "`t", 9
        $f[1] -eq $RegionalCulture -and ($f[8] -match 'U\+00A0|U\+202F' -or ($RegionalCulture -eq 'de-DE' -and $f[8].Contains('/')))
    })
    if (-not $rows.Count) { throw "No affected cases for $RegionalCulture" }
    $app.UseSystemSeparators = $true
    $first = $rows[0] -split "`t", 9
    if ($app.International(3) -ne (Literal $first[2]) -or $app.International(4) -ne (Literal $first[3])) {
        throw 'Excel regional decimal or thousands separator differs from the selected questions.'
    }
}
$regional = if ($RegionalCulture) { [ordered]@{
    culture = (Get-Culture).Name; excel_version = $app.Version; excel_build = $app.Build
    country_setting = $app.International(2); decimal = $app.International(3)
    thousands = $app.International(4); date_separator = $app.International(17)
} } else { $null }
$out = New-Object System.Collections.Generic.List[string]
$out.Add("# id`tculture`tdecimal`tthousands`tlcid`tdate_system`tkind`tvalue`tcode`ttext`tinvariant_code`terror")
$groups = $rows | ForEach-Object { $f = $_ -split "`t"; [pscustomobject]@{ row = $_; key = ($f[2] + '|' + $f[3] + '|' + $f[5]) } } | Group-Object key
    foreach ($group in $groups) {
        $first = $group.Group[0].row -split "`t"
        if (-not $RegionalCulture) {
            $app.UseSystemSeparators = $false
            $app.DecimalSeparator = Literal $first[2]
            $app.ThousandsSeparator = Literal $first[3]
        }
        $book = $app.Workbooks.Add()
        $book.Date1904 = ($first[5] -eq '1904')
        $sheet = $book.Worksheets.Item(1)
        $sheet.Columns.ColumnWidth = 60
        foreach ($item in $group.Group) {
            $f = $item.row -split "`t", 9
            $text = ''; $invariant = ''; $err = ''
            try {
                $cell = $sheet.Range('A1')
                $cell.ClearContents() | Out-Null
                $cell.NumberFormat = 'General'
                $prefix = ''
                if ($f[4]) { $prefix = '[$-' + $f[4] + ']' }
                $code = Literal $f[8]
                if ($f[6] -eq 'fn') {
                    # The cell's formula is TEXT(value, code): the code in the culture's separators.
                    $cell.Formula = '=TEXT(' + $f[7] + ',"' + ($code -replace '"', '""') + '")'
                    $cell.Calculate() | Out-Null
                } elseif ($f[6] -eq 'text') {
                    $cell.NumberFormat = '@'
                    $cell.Formula = $f[7]
                    $cell.NumberFormatLocal = $prefix + $code
                } else {
                    $cell.NumberFormatLocal = $prefix + $code
                    $cell.Value2 = [double]::Parse($f[7], [Globalization.CultureInfo]::InvariantCulture)
                }
                $text = $cell.Text
                $invariant = $cell.NumberFormat
            } catch { $err = $_.Exception.Message -replace "[\t\r\n]+", ' ' }
            $out.Add(($f[0], $f[1], $f[2], $f[3], $f[4], $f[5], $f[6], $f[7], $f[8], (Show $text), (Show $invariant), $err) -join "`t")
        }
        $book.Close($false)
        $book = $null
    }
} finally {
    if ($book) { try { $book.Close($false) } catch { } }
    $app.UseSystemSeparators = $before.system
    if (-not $before.system) { $app.DecimalSeparator = $before.decimal; $app.ThousandsSeparator = $before.thousands }
}
$sorted = @($out[0]) + @($out | Select-Object -Skip 1 | Sort-Object { [int](($_ -split "`t")[0] -replace '^\D+', '') })
[IO.File]::WriteAllLines($Output, $sorted, (New-Object System.Text.UTF8Encoding($false)))
@{fatal = $null; regional = $regional; rows = @(@{cases = $rows.Count; answered = $out.Count - 1})} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $Result -Encoding utf8NoBOM
