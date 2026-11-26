param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
function TryEdit([scriptblock]$body) {
    try {@{ok=$true;value=(& $body)}} catch {@{ok=$false;hr=('0x{0:X8}' -f $_.Exception.HResult);description=$_.Exception.Message}}
}
foreach($mode in 'default','allow','uiOnly') {
    $b=Book
    try {
        $s=$b.Worksheets.Item(1);$s.Name='Data'
        $s.Range('A1').Value2='Kind';$s.Range('B1').Value2='Qty'
        $s.Range('A2').Value2='a';$s.Range('B2').Value2=3
        $s.Range('A3').Value2='b';$s.Range('B3').Value2=5
        $s.Range('B2').Locked=$false
        $cache=$b.PivotCaches().Create(1,'Data!R1C1:R3C2',8)
        $p=$cache.CreatePivotTable($s.Range('E3'),'LanePivot',$true,8)
        $p.PivotFields('Kind').Orientation=1
        $null=$p.AddDataField($p.PivotFields('Qty'),'Sum Qty',-4157)
        $argsList=@('', $true,$true,$true,($mode -eq 'uiOnly'))+(@($false)*10)+@($mode -eq 'allow')
        $p0=$argsList
        $s.Protect($p0[0],$p0[1],$p0[2],$p0[3],$p0[4],$p0[5],$p0[6],$p0[7],$p0[8],$p0[9],$p0[10],$p0[11],$p0[12],$p0[13],$p0[14],$p0[15])
        Observe "AllowPivot.$mode" {
            $s.Range('B2').Value2=9
            $before=@(Numbers $s.Range('E3:F6'))
            $refresh=TryEdit {$p.RefreshTable()}
            $after=@(Numbers $s.Range('E3:F6'))
            $orientation=TryEdit {$p.PivotFields('Kind').Orientation=2; $p.PivotFields('Kind').Orientation}
            $b.SaveAs("$Directory/pivot-$mode.xlsx",51)
            @{flag=$s.Protection.AllowUsingPivotTables;before=$before;refresh=$refresh;after=$after;orientation=$orientation}
        }
    } finally {$b.Close($false)}
}
