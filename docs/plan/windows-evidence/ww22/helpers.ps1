$ErrorActionPreference='Stop'
$excel=$OwnedExcel
$missing=[Type]::Missing
$image=(Get-Item (Join-Path $excel.Path 'EXCEL.EXE')).VersionInfo.FileVersion
$meta=[ordered]@{excel_build=$image;cpu=(Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name).Trim(); timestamp_utc=[DateTime]::UtcNow.ToString('o');platform='Windows';bitness='64';lane='ww22'}
$meta | ConvertTo-Json | Set-Content "$Directory/provenance.json"
function Observe($Id,[scriptblock]$Body) {
    try {
        $row=[ordered]@{id=$Id;ok=$true;value=(& $Body)}
    } catch {
        $e=$_.Exception
        while ($e.InnerException) { $e=$e.InnerException }
        $row=[ordered]@{id=$Id;ok=$false;hresult=('0x{0:X8}' -f $e.HResult);description=$e.Message}
    }
    $row | ConvertTo-Json -Compress -Depth 12 | Add-Content "$Directory/observations.jsonl"
}
function Book { $excel.Workbooks.Add(-4167) }
function Tabs($book) { for ($i=1;$i -le $book.Sheets.Count;$i++) { $book.Sheets.Item($i).Name } }
function Group($book,[object[]]$names) { $book.Worksheets.Item($names) }
function Numbers($range) {
    foreach($c in $range.Cells) { [ordered]@{address=$c.Address();value=$c.Value2;text=$c.Text;formula=$c.Formula;format=$c.NumberFormat} }
}
