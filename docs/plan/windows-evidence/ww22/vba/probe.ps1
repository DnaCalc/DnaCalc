param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
function Module($book,[string]$source) {
    $m=$book.VBProject.VBComponents.Add(1)
    $m.Name='LaneProbe'
    $m.CodeModule.AddFromString($source)
}
foreach($case in 'before-inside','after-inside','reversed','group-no-anchor') {
    $b=Book
    try {
        $d=$b.Worksheets.Item(1); $d.Name='D'
        $c=$b.Worksheets.Add($d); $c.Name='C'
        $bs=$b.Worksheets.Add($c); $bs.Name='B'
        $a=$b.Worksheets.Add($bs); $a.Name='A'
        $operation=switch($case) {
            'before-inside' {'Worksheets(Array(2, 3)).Move Before:=Worksheets(2)'}
            'after-inside' {'Worksheets(Array(2, 3)).Move After:=Worksheets(3)'}
            'reversed' {'Worksheets(Array(3, 2)).Move Before:=Worksheets(1)'}
            'group-no-anchor' {'Worksheets(Array(3, 2)).Move'}
        }
        Module $b @"
Public Function Probe() As String
On Error GoTo Failed
$operation
Probe = "OK"
Exit Function
Failed:
Probe = CStr(Err.Number) & "|" & Err.Description
End Function
"@
        Observe "VBA.Move.$case" {
            $ret=$excel.Run("'$($b.Name)'!LaneProbe.Probe")
            $active=$excel.ActiveWorkbook
            $row=@{vba=$ret;source=@(Tabs $b);active=@(Tabs $active);name=$active.Name;sheet=$excel.ActiveSheet.Name}
            if($active.Name -ne $b.Name) {$active.Close($false)}
            $row
        }
    } finally { $b.Close($false) }
}
$b=Book
try {
    Module $b @"
Public Function Probe(kind As Long) As String
Dim x As Variant
Dim a() As Long
On Error GoTo Failed
Select Case kind
Case 1
 x = Application.Match(9, Array(3, 5), 0)
 Probe = TypeName(x) & "|" & CStr(x)
Case 2
 Probe = CStr(Application.Match(5, Array(3, 5, 7), 0))
Case 3
 ActiveSheet.Range("C2").Value2 = 7
 Probe = CStr(Application.CountA(ActiveSheet.Range("C1:C3")))
Case 4
 x = Application.Transpose(Array(8, 9))
 Probe = CStr(x(2, 1))
Case 5
 Probe = CStr(Application.WorksheetFunction.Match(9, Array(3, 5), 0))
Case 6
 ReDim a(1 To 2) As Long
 a(1) = 7
 ReDim Preserve a(1 To 3) As Long
 Probe = CStr(a(1)) & "|" & CStr(UBound(a))
Case 7
 Application.Toolbars("Standard").Visible = True
 Probe = "OK"
Case 8
 Application.CommandBars("Cell").Enabled = True
 Probe = "OK"
Case 9
 ActiveSheet.Shapes("nosuch").Delete
End Select
Exit Function
Failed:
Probe = CStr(Err.Number) & "|" & Err.Description
End Function
"@
    foreach($i in 1..9) { Observe "VBA.Member.$i" { $excel.Run("'$($b.Name)'!LaneProbe.Probe",$i) } }
    Observe 'Paste.Enum.21' {
        $appType=$excel.GetType()
        $typeLibrary=(Get-Item (Join-Path $excel.Path 'EXCEL.EXE')).FullName
        @{library=$typeLibrary}
    }
} finally {$b.Close($false)}
foreach($method in 'Calculate','CalculateFull','CalculateFullRebuild','BeforeSaveClose') {
    $first=Book; $second=Book
    try {
        foreach($b in $first,$second) {
            $s=$b.Worksheets.Item(1)
            $b.VBProject.VBComponents.Item('ThisWorkbook').CodeModule.AddFromString(@"
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
            Module $b @"
Public Function RunCalc(ByVal method As String) As String
On Error GoTo Failed
Select Case method
Case "Calculate": Application.Calculate
Case "CalculateFull": Application.CalculateFull
Case "CalculateFullRebuild": Application.CalculateFullRebuild
Case "BeforeSaveClose": ThisWorkbook.Close True
End Select
RunCalc = "OK"
Exit Function
Failed:
RunCalc = CStr(Err.Number) & "|" & Err.Description
End Function
"@
            $s.Range('A1').Value2=2
            $s.Range('B1').Formula='=A1*2'
            $s.Range('C1').Formula='=A1+10'
        }
        $excel.Calculate()
        $excel.Calculation=-4135
        $first.Worksheets.Item(1).Range('A1').Value2=5
        $first.Worksheets.Item(1).Range('F1:F2').ClearContents()
        $second.Worksheets.Item(1).Range('F1:F2').ClearContents()
        $second.Activate()
        Observe "VBA.MultiBook.$method" {
            $ret=$excel.Run("'$($first.Name)'!LaneProbe.RunCalc",$method)
            @{vba=$ret;number_of_books=$excel.Workbooks.Count;active=$excel.ActiveWorkbook.Name;first=$first.Name;second=$second.Name;first_cells=@(Numbers $first.Worksheets.Item(1).Range('A1:C1'))+@(Numbers $first.Worksheets.Item(1).Range('F1:F2'));second_cells=@(Numbers $second.Worksheets.Item(1).Range('A1:C1'))+@(Numbers $second.Worksheets.Item(1).Range('F1:F2'))}
        }
    } finally { try {$first.Close($false)} catch {}; $second.Close($false); $excel.Calculation=-4105 }
}
