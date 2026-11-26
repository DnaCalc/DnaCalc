param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$first=Book; $second=Book
try {
    $excel.EnableEvents=$true
    foreach($b in $first,$second) {
        $s=$b.Worksheets.Item(1)
        $b.VBProject.VBComponents.Item($b.CodeName).CodeModule.AddFromString(@"
Private Sub Workbook_SheetCalculate(ByVal Sh As Object)
With Sh.Range("F1")
.Value2 = .Value2 + 1
End With
End Sub
Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)
With Me.Worksheets(1).Range("F2")
.Value2 = .Value2 + 1
End With
Cancel = True
End Sub
"@)
        $s.Range('A1').Value2=2
        $s.Range('B1').Formula='=A1*2'
        $s.Range('C1').Formula='=A1+10'
    }
    foreach($method in 'Calculate','CalculateFull','CalculateFullRebuild') {
        $excel.Calculation=-4105
        $excel.Calculate()
        $excel.Calculation=-4135
        $s1=$first.Worksheets.Item(1);$s2=$second.Worksheets.Item(1)
        $s1.Range('A1').Value2=$s1.Range('A1').Value2+1
        $null=$s1.Range('F1:F2').ClearContents()
        $null=$s2.Range('F1:F2').ClearContents()
        $second.Activate()
        Observe "MultiBook.$method" {
            $before=@{nonActive=$s1.Range('B1').Value2;active=$s2.Range('B1').Value2}
            switch($method) {'Calculate' {$excel.Calculate()}; 'CalculateFull' {$excel.CalculateFull()};'CalculateFullRebuild' {$excel.CalculateFullRebuild()}}
            @{before=$before;active=$excel.ActiveWorkbook.Name;first=$first.Name;second=$second.Name;nonActive=$s1.Range('B1').Value2;activeValue=$s2.Range('B1').Value2;nonActiveEvents=$s1.Range('F1').Value2;activeEvents=$s2.Range('F1').Value2}
        }
    }
    $excel.EnableEvents=$false
    $first.SaveAs("$Directory/before-save.xlsm",52)
    $excel.EnableEvents=$true
    $s1.Range('D1').Value2=42
    Observe 'MultiBook.CloseSaveCancelled' {
        $ret=$first.Close($true)
        @{result=$ret;books=$excel.Workbooks.Count;open=@($excel.Workbooks | ForEach-Object Name);saved=$first.Saved;target=$s1.Range('D1').Value2;beforeSaveEvents=$s1.Range('F2').Value2}
    }
} finally {
    $excel.EnableEvents=$false
    $excel.Calculation=-4105
    try {$first.Close($false)} catch {}
    try {$second.Close($false)} catch {}
}
