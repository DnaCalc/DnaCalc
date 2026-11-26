param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$flags='FormattingCells','FormattingColumns','FormattingRows','InsertingColumns','InsertingRows','InsertingHyperlinks','DeletingColumns','DeletingRows','Sorting','Filtering','UsingPivotTables'
function TryEdit([scriptblock]$action) {
    try {$null=& $action; @{ok=$true}} catch {@{ok=$false;hr=('0x{0:X8}' -f $_.Exception.HResult);message=$_.Exception.Message}}
}
foreach($i in 0..10) {
    $b=Book
    try {
        $s=$b.Worksheets.Item(1)
        $s.Range('E1').Value2='Qty';$s.Range('E2').Value2=3;$s.Range('E3').Value2=1
        $null=$s.Range('E1:E3').AutoFilter()
        $s.Range('A1:A5').Locked=$false; $s.Range('B:B').Locked=$false; $s.Rows.Item(6).Locked=$false
        $argsList=@($false)*11; $argsList[$i]=$true
        $mod=$b.VBProject.VBComponents.Add(1)
        $assign="Allow$($flags[$i]):=True"
        $mod.CodeModule.AddFromString(@"
Public Sub Apply()
ActiveSheet.Protect Password:="abc", $assign
End Sub
"@)
        $path="$Directory/allow-$($flags[$i]).xlsx"
        Observe "Allow.$($flags[$i])" {
            $null=$excel.Run("'$($b.Name)'!Apply")
            $prop="Allow$($flags[$i])"
            $before=$s.Protection.$prop
            $edit=switch($i) {
                0 {TryEdit {$s.Range('C1').Font.Bold=$true}}
                1 {TryEdit {$s.Columns.Item(3).ColumnWidth=18}}
                2 {TryEdit {$s.Rows.Item(8).RowHeight=25}}
                3 {TryEdit {$s.Columns.Item(2).Insert()}}
                4 {TryEdit {$s.Rows.Item(6).Insert()}}
                5 {TryEdit {$s.Hyperlinks.Add($s.Range('A3'),'https://example.invalid/')}}
                6 {TryEdit {$s.Columns.Item(2).Delete()}}
                7 {TryEdit {$s.Rows.Item(6).Delete()}}
                8 {TryEdit {$s.Range('E2:E3').Sort($s.Range('E2'),1)}}
                9 {TryEdit {$s.Range('E1:E3').AutoFilter(1,'>1')}}
                10 { @{not_run='Pivot fixture required'} }
            }
            $negative=TryEdit {$s.Range('C1').Value2='blocked'}
            $b.SaveAs($path,51);$b.Close($false)
            $r=$excel.Workbooks.Open($path)
            try {
                $ws=$r.Worksheets.Item(1)
                @{before=$before;reopened=$ws.Protection.$prop;edit=$edit;locked_write=$negative;contents=$ws.ProtectContents}
            } finally {$r.Close($false)}
        }
    } finally {try {$b.Close($false)} catch {}}
}
foreach($case in 'same','wrong','omitted') {
    $b=Book
    try {
        $s=$b.Worksheets.Item(1); $s.Protect('abc')
        Observe "Protect.Again.$case" {
            $pass=switch($case){'same'{'abc'};'wrong'{'wrong'};'omitted'{$missing}}
            $change=TryEdit {$s.Protect($pass,$true,$true,$true,$true)}
            $write=TryEdit {$s.Range('A1').Value2=3}
            @{change=$change;write=$write;mode=$s.ProtectionMode;contents=$s.ProtectContents}
        }
    } finally {$b.Close($false)}
}
