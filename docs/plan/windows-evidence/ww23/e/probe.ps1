param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$vba=@'
Dim v As Long
Dim Static1 As Long
Private Function Hid(ws As Object, a As Long, b As Long) As String
 Dim i As Long, t As String
 For i = a To b: t = t & IIf(ws.Rows(i).Hidden, "H", "v"): Next
 Hid = t
End Function
Private Sub Seed()
 With ActiveSheet
  .Cells.Clear
  .Range("A1").Value = "name": .Range("B1").Value = "qty"
  .Range("A2").Value = "a": .Range("B2").Value = 5
  .Range("A3").Value = "b": .Range("B3").Value = 15
  .Range("A4").Value = "c": .Range("B4").Value = 25
  .Range("A5").Value = "d": .Range("B5").Value = 35
 End With
End Sub
Public Function Probe(k As Long) As String
 Dim ws As Object, t As String, r As Range
 On Error GoTo Failed
 Set ws = ActiveSheet
 Select Case k
 Case 1
  Call Seed
  ws.Range("A1").AutoFilter
  Probe = "on=" & ws.AutoFilterMode & " rng=" & ws.AutoFilter.Range.Address
  ws.Range("A1").AutoFilter
  Probe = Probe & " second: on=" & ws.AutoFilterMode
 Case 2
  Call Seed
  ws.Range("A1:B5").AutoFilter Field:=2, Criteria1:=">10", Operator:=xlAnd, Criteria2:="<30"
  Probe = Hid(ws, 1, 5) & " filtermode=" & ws.FilterMode
 Case 3
  Call Seed
  ws.Range("A1:B5").AutoFilter Field:=2, Criteria1:=2, Operator:=xlTop10Items
  Probe = Hid(ws, 1, 5)
 Case 4
  Call Seed
  ws.Range("A1:B5").AutoFilter Field:=2, Criteria1:=Array("5", "35"), Operator:=xlFilterValues
  Probe = Hid(ws, 1, 5)
 Case 5
  Call Seed
  ws.Range("A1:B5").AutoFilter
  ws.ShowAllData
  Probe = "ok fm=" & ws.FilterMode
 Case 6
  Call Seed
  ws.AutoFilterMode = True
  Probe = "ok"
 Case 7
  Call Seed
  ws.Range("A1:B5").AutoFilter
  Probe = "fm0=" & ws.FilterMode
  ws.Range("A1:B5").AutoFilter Field:=2, Criteria1:=">10"
  Probe = Probe & " fm1=" & ws.FilterMode
  ws.ShowAllData
  Probe = Probe & " fm2=" & ws.FilterMode & " mode=" & ws.AutoFilterMode
  ws.AutoFilterMode = False
  Probe = Probe & " afterOff=" & ws.AutoFilterMode
 Case 8
  Call Seed
  ws.Range("A1:B5").AutoFilter Field:=3, Criteria1:="x"
  Probe = "ok"
 Case 9
  Call Seed
  ws.Range("A1:B5").AutoFilter Field:=2, Criteria1:="1", Operator:=20
  Probe = "ok"
 Case 10
  Call Seed
  ws.Range("A1:B5").AutoFilter Field:=2, Criteria1:=0, Operator:=xlTop10Items
  Probe = "ok"
 Case 11
  Call Seed
  ws.Range("A1:B5").AutoFilter Field:=2, Criteria1:=501, Operator:=xlTop10Items
  Probe = "ok " & Hid(ws, 1, 5)
 Case 12
  Call Seed
  ws.Range("A1:B5").AutoFilter Field:=2, Criteria1:=500, Operator:=xlTop10Items
  Probe = "ok " & Hid(ws, 1, 5)
 Case 20
  Call Seed
  ws.Range("B3").Sort Key1:=ws.Range("B2"), Order1:=xlDescending
  Probe = ws.Range("A1").Value & ws.Range("A2").Value & ws.Range("A3").Value & ws.Range("A4").Value & ws.Range("A5").Value & " b1=" & ws.Range("B1").Value
 Case 21
  ws.Cells.Clear: ws.Range("H9").Value = 5
  ws.Range("H9").Sort Key1:=ws.Range("H9")
  Probe = "ok"
 Case 30
  ws.Cells.Clear
  ws.Range("A1").Value = "": ws.Range("A2").Value = 7: ws.Range("B1").Value = 3: ws.Range("B2").Value = 4
  ws.Range("A1").Clear
  ws.Range("B1:B2").Copy
  ws.Range("A1:A2").PasteSpecial Paste:=xlPasteValues, Operation:=xlAdd
  Probe = "blankdest add: " & ws.Range("A1").Value2 & "," & ws.Range("A2").Value2
 Case 31
  ws.Cells.Clear
  ws.Range("A1").Value = 5: ws.Range("A2").Value = 6
  ws.Range("D1:D2").ClearContents
  ws.Range("D1:D2").Copy
  Dim ops, o
  ops = Array(xlAdd, xlSubtract, xlMultiply, xlDivide)
  For Each o In ops
   ws.Range("A1").Value = 5: ws.Range("A2").Value = 6
   ws.Range("D1:D2").Copy
   ws.Range("A1:A2").PasteSpecial Paste:=xlPasteValues, Operation:=o
   t = t & o & ":" & ws.Range("A1").Text & "," & ws.Range("A2").Text & ";"
  Next
  Probe = "blank src on numbers: " & t
 Case 32
  ws.Cells.Clear
  ws.Range("A1").Value = "txt": ws.Range("A2").Formula = "=1+1"
  ws.Range("D1").Value = 2: ws.Range("D2").Value = 3
  ws.Range("D1:D2").Copy
  ws.Range("A1:A2").PasteSpecial Paste:=xlPasteValues, Operation:=xlAdd
  Probe = "text dest add: " & ws.Range("A1").Text & " | formula dest: " & ws.Range("A2").Formula & " " & ws.Range("A2").Text
 Case 33
  ws.Cells.Clear
  ws.Range("A1").Value = 1: ws.Range("A2").Value = 1
  ws.Range("D1").Value = 0: ws.Range("D2").Value = 4
  ws.Range("D1:D2").Copy
  ws.Range("A1:A2").PasteSpecial Paste:=xlPasteAll, Operation:=xlDivide
  Probe = "div zero: " & ws.Range("A1").Text & "," & ws.Range("A2").Text & " fm " & ws.Range("A1").Formula
 Case 34
  ws.Cells.Clear
  ws.Range("A1").Value = 3: ws.Range("A1").Interior.Color = 255: ws.Range("B1").Value = 4
  ws.Range("A1:B1").Copy
  ws.Range("D1:E1").Value = 10
  ws.Range("D1:E1").PasteSpecial Paste:=xlPasteAll, Operation:=xlAdd
  Probe = "all+add: " & ws.Range("D1").Value & "," & ws.Range("E1").Value & " fill " & ws.Range("D1").Interior.Color
 Case 35
  ws.Cells.Clear
  ws.Range("A1").Value = 3: ws.Range("B1").Interior.Color = 255
  ws.Range("A1:B1").Copy
  ws.Range("D1:E1").PasteSpecial Paste:=xlPasteAll, SkipBlanks:=True
  Probe = "skipblanks all: E1 fill " & ws.Range("E1").Interior.Color
 Case 36
  ws.Cells.Clear
  ws.Range("A1").Value = 1: ws.Range("B1").Value = 2: ws.Range("A2").Value = 3: ws.Range("B2").Value = 4
  ws.Range("A1:B2").Copy
  ws.Range("D1").PasteSpecial Paste:=xlPasteAll, Transpose:=True, SkipBlanks:=True
  Probe = "transp: " & ws.Range("D1").Value & ws.Range("E1").Value & ws.Range("D2").Value & ws.Range("E2").Value
 Case 37
  ws.Cells.Clear
  ws.Range("A1").Value = 1
  ws.Range("A1").Copy
  ws.PasteSpecial Format:="Text"
  Probe = "ws.PasteSpecial text ok sel=" & Selection.Address & " " & ActiveCell.Value
 Case 38
  ws.Cells.Clear
  ws.Range("A1").Value = 1: ws.Range("A1").Copy
  ws.Range("C3").Select
  ws.Paste
  Probe = "ws.Paste ok " & ws.Range("C3").Value
 Case 40
  v = 5: Probe = "set v"
  End
 Case 41
  Probe = "v=" & v & " enable=" & Application.EnableEvents & " su=" & Application.ScreenUpdating & " wb=" & Workbooks.Count
 Case 42
  Static1 = 7
  Application.EnableEvents = False
  Application.ScreenUpdating = False
  Probe = "x"
  Call EndInner
 End Select
 Exit Function
Failed:
 Probe = Probe & " ERR " & Err.Number & "|" & Err.Description
End Function
Private Sub EndInner()
 End
End Sub
Public Function Probe2() As String
 Probe2 = "static=" & Static1 & " v=" & v & " ee=" & Application.EnableEvents & " su=" & Application.ScreenUpdating & " wbs=" & Workbooks.Count
 Application.EnableEvents = True: Application.ScreenUpdating = True
End Function
'@
foreach($k in 1,2,3,4,5,6,7,8,9,10,11,12,20,21,30,31,32,33,34,35,36,37,38){
 $b=Book
 try {
  $m=$b.VBProject.VBComponents.Add(1); $m.Name='LaneProbe'; $m.CodeModule.AddFromString($vba)
  Observe "VBA.e$k" { @{vba=$excel.Run("'$($b.Name)'!LaneProbe.Probe",[int]$k)} }
 } finally { foreach($x in @($excel.Workbooks)){ try{$x.Close($false)}catch{} } }
}
# End semantics
$b=Book
try {
 $m=$b.VBProject.VBComponents.Add(1); $m.Name='LaneProbe'; $m.CodeModule.AddFromString($vba)
 Observe 'VBA.End.first' { @{r=$excel.Run("'$($b.Name)'!LaneProbe.Probe",40)} }
 Observe 'VBA.End.second' { @{r=$excel.Run("'$($b.Name)'!LaneProbe.Probe",41)} }
 Observe 'VBA.End.inner' { @{r=$excel.Run("'$($b.Name)'!LaneProbe.Probe",42)} }
 Observe 'VBA.End.afterInner' { @{r=$excel.Run("'$($b.Name)'!LaneProbe.Probe2")} }
 $excel.EnableEvents=$false
} finally { foreach($x in @($excel.Workbooks)){ try{$x.Close($false)}catch{} } }
