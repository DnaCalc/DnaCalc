param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
foreach($criterion in 'known','unknown','computed') {
    foreach($header in 'blank','known') {
        $b=Book
        try {
            $s=$b.Worksheets.Item(1)
            $s.Range('A1').Value2='Item';$s.Range('B1').Value2='Qty'
            $s.Range('A2').Value2='a';$s.Range('A3').Value2='b';$s.Range('A4').Value2='c'
            $s.Range('B2').Value2=5;$s.Range('B3').Value2=15;$s.Range('B4').Value2=25
            $s.Range('G2:G6').Value2='old';$s.Range('G8').Value2='isolated';$s.Range('G1:H8').NumberFormat='0.000'
            $s.Range('H2').Value2='near'
            if($header -eq 'known') {$s.Range('G1').Value2='Item'}
            switch($criterion) {
                'known' {$s.Range('D1').Value2='Qty'; $s.Range('D2').Value2='>20'}
                'unknown' {$s.Range('D1').Value2='Other'; $s.Range('D2').Value2='>20'}
                'computed' {$s.Range('D1').Value2='Other'; $s.Range('D2').Formula='=B2>20'}
            }
            Observe "AdvancedFilter.Stale.$criterion.$header" {
                $ret=$s.Range('A1:B4').AdvancedFilter(2,$s.Range('D1:D2'),$s.Range('G1'))
                @{returned=$ret;cells=@(Numbers $s.Range('G1:H8'))}
            }
        } finally {$b.Close($false)}
    }
}
foreach($case in 'active-left','inactive-left','active-right','inactive-right') {
    $b=Book
    try {
        $a=$b.Worksheets.Item(1); $a.Name='A'
        $h=$b.Worksheets.Add($missing,$a);$h.Name='Hidden'
        $c=$b.Worksheets.Add($missing,$h);$c.Name='C'
        $h.Visible=0
        if($case -in 'active-left','inactive-right') {$c.Activate()} else {$a.Activate()}
        Observe "HiddenTab.Move.$case" {
            $before=@{tabs=@(Tabs $b);active=$excel.ActiveSheet.Name}
            if($case.EndsWith('left')) {$c.Move($a)} else {$a.Move($missing,$c)}
            @{before=$before;after=@(Tabs $b);active=$excel.ActiveSheet.Name;hidden=$h.Visible}
        }
    } finally {$b.Close($false)}
}
foreach($case in 'embedded-simple','embedded-xml','sheet') {
    $b=$excel.Workbooks.Open("$PSScriptRoot/../charts/base.xlsx")
    try {
        $m=$b.VBProject.VBComponents.Add(1)
        $action=switch($case) {
            'embedded-simple' {'Set ch = Worksheets(1).ChartObjects(1).Chart: ch.Name = "Simple"'}
            'embedded-xml' {'Set ch = Worksheets(1).ChartObjects(1).Chart: ch.Name = "New & <First>"'}
            'sheet' {'Set ch = Charts.Add: ch.Name = "Renamed Chart"'}
        }
        $m.CodeModule.AddFromString(@"
Public Function RenameChart() As String
Dim ch As Chart
On Error GoTo Failed
$action
RenameChart = ch.Name
Exit Function
Failed:
RenameChart = CStr(Err.Number) & "|" & Err.Description
End Function
"@)
        Observe "VBA.Chart.Name.$case" {$excel.Run("'$($b.Name)'!RenameChart")}
    } finally {$b.Close($false)}
}
