param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
foreach($count in 0,-1,3,256) {
    $b=Book
    try {
        $s=$b.Worksheets.Item(1);$s.Name='Data'
        $m=$b.VBProject.VBComponents.Add(1)
        $m.CodeModule.AddFromString(@"
Public Function AddCount(ByVal n As Long) As String
Dim ws As Object
On Error GoTo Failed
Set ws = Worksheets.Add(Count:=n)
AddCount = TypeName(ws) & "|" & ws.Name
Exit Function
Failed:
AddCount = CStr(Err.Number) & "|" & Err.Description
End Function
"@)
        Observe "Worksheets.Add.Count.$count" {
            $v=$excel.Run("'$($b.Name)'!AddCount",$count)
            $tabs=@(Tabs $b)
            @{vba=$v;count=$tabs.Count;first=@($tabs|Select-Object -First 5);last=@($tabs|Select-Object -Last 3);active=$excel.ActiveSheet.Name}
        }
    } finally {$b.Close($false)}
}
foreach($mode in 'single-source','skip-blanks','outline','source10-cut') {
    $b=Book
    try {
        $s=$b.Worksheets.Item(1)
        $s.Range('B2').Value2=5;$s.Range('E1:H3').Value2=7
        $s.Columns.Item('A:A').ColumnWidth=20;$s.Columns.Item('B:B').ColumnWidth=30
        if($mode -eq 'outline') {$null=$s.Columns.Item('A:B').Group();$s.Columns.Item('B:B').Hidden=$true}
        $s.Columns.Item('E:H').ColumnWidth=12
        Observe "Widths.$mode" {
            if($mode -eq 'source10-cut') {
                $null=$s.Range('A1:B2').Cut()
            } else {
                $source=if($mode -eq 'single-source') {'A1:A2'} else {'A1:B2'}
                $null=$s.Range($source).Copy()
            }
            try {$ret=$s.Range('E1:H1').PasteSpecial(8,$missing,($mode -eq 'skip-blanks'));$err=$null}
            catch {$ret=$null;$err=@{hr=('0x{0:X8}' -f $_.Exception.HResult);text=$_.Exception.Message}}
            $cols=foreach($l in 'A','B','E','F','G','H') {
                $col=$s.Columns.Item("$($l):$l")
                $hidden=$col.Hidden;$col.Hidden=$false
                @{column=$l;unhidden=$col.ColumnWidth;hidden=$hidden;outline=$col.OutlineLevel}
            }
            @{returned=$ret;error=$err;columns=@($cols);cells=@(Numbers $s.Range('E1:H1'))}
        }
    } finally {$excel.CutCopyMode=$false;$b.Close($false)}
}
foreach($operation in 2..5) {
    $b=Book
    try {
        $s=$b.Worksheets.Item(1); $s.Range('A1').Value2='source';$s.Range('A2').Value2=2;$s.Range('A3').Value2=2;$s.Range('A4').Value2=0
        $s.Range('A1:A4').NumberFormat='0.000';$s.Range('A1:A4').Font.Bold=$true
        $s.Range('E1').Value2=7;$s.Range('E2').Formula='=2+3';$s.Range('E3').Value2='original';$s.Range('E4').Value2=9
        Observe "Paste.Operation.FormulaText.$operation" {
            $null=$s.Range('A1:A4').Copy()
            $ret=$s.Range('E1').PasteSpecial(-4104,$operation)
            @{returned=$ret;cells=@(Numbers $s.Range('E1:E4'));bold=$s.Range('E1:E4').Font.Bold}
        }
    } finally {$excel.CutCopyMode=$false;$b.Close($false)}
}
