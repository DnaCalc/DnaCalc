Private Function Er() As String
 Er = "ERR " & Err.Number & "|" & Err.Description
End Function
Private Function Tabs() As String
 Dim s As Object, t As String
 For Each s In ActiveWorkbook.Sheets: t = t & s.Name & ",": Next
 Tabs = t
End Function
Private Sub Fill()
 With ActiveSheet
 .Cells.Clear
 .Range("A1").Value = "apple": .Range("B1").Value = "pie": .Range("C1").Value = "x"
 .Range("A2").Value = "Apple pie": .Range("B2").Value = "fig": .Range("C2").Value = 12.5
 .Range("A3").Value = "banana": .Range("B3").Value = "APPLE": .Range("C3").Formula = "=1+1"
 End With
End Sub
Public Function Probe(k As Long) As String
 Dim r As Range, f As Range, w As Variant, ws As Object, b As Workbook, n As Name, i As Long, t As String
 On Error GoTo Failed
 Select Case k
 Case 1
  Call Fill: Set r = ActiveSheet.Range("A1:C3")
  Set f = r.Find("apple", After:=ActiveSheet.Range("E5"))
  Probe = "hit " & f.Address
 Case 2
  Call Fill: Set r = ActiveSheet.Range("A1:C3")
  Set f = r.Find("apple", After:=ActiveSheet.Range("A1:B2"))
  Probe = "hit " & f.Address
 Case 3
  Call Fill: Set r = ActiveSheet.Range("A1:C3")
  Set f = r.Find(w)
  If f Is Nothing Then Probe = "Nothing" Else Probe = "hit " & f.Address
 Case 4
  Call Fill: ActiveSheet.Range("D1").Formula = "=""""": 
  Set f = ActiveSheet.Range("D1:D2").Find("*", LookIn:=xlValues)
  If f Is Nothing Then Probe = "Nothing" Else Probe = "hit " & f.Address
 Case 5
  Set f = ActiveSheet.Range("A1:C3").FindNext
  If f Is Nothing Then Probe = "Nothing" Else Probe = "hit " & f.Address
 Case 6
  Set ws = ActiveWorkbook.Sheets.Add(Type:=xlChart)
  Probe = "OK " & TypeName(ws) & " " & ws.Name & " " & Tabs()
 Case 7
  Set ws = ActiveWorkbook.Worksheets.Add(Type:=xlChart)
  Probe = "OK " & TypeName(ws) & " " & ws.Name & " " & Tabs()
 Case 8
  Set ws = ActiveWorkbook.Worksheets.Add(Count:=2, Type:=xlWorksheet)
  Probe = "OK " & ws.Name & " " & Tabs()
 Case 10
  Set n = ActiveWorkbook.Names.Add(Name:="Sheet1!Local", RefersTo:="=1")
  Probe = "OK " & n.Name & "|" & TypeName(n.Parent) & "|" & n.Parent.Name & "|" & ActiveWorkbook.Names.Count
 Case 11
  ActiveWorkbook.Worksheets.Add.Name = "My Sheet"
  Set n = ActiveWorkbook.Names.Add(Name:="'My Sheet'!Local", RefersTo:="=2")
  Set n = ActiveWorkbook.Names.Add(Name:="Zed", RefersTo:="=3")
  Set n = ActiveWorkbook.Names.Add(Name:="Alpha", RefersTo:="=4")
  For i = 1 To ActiveWorkbook.Names.Count: t = t & ActiveWorkbook.Names(i).Name & ";": Next
  Probe = "Names " & ActiveWorkbook.Names.Count & ": " & t
 Case 12
  Set n = ActiveWorkbook.Names.Add(Name:="NoSuch!Local", RefersTo:="=1")
  Probe = "OK " & n.Name
 Case 13
  Set n = ActiveWorkbook.Names.Item("Local")
  Probe = "OK " & n.Name
 Case 14
  Set n = ActiveWorkbook.Names.Item("Sheet1!Local")
  Probe = "OK " & n.Name & "|" & n.RefersTo
 Case 20
  ActiveSheet.Name = "Sheet1": ActiveSheet.Range("A1").Value = 5: ActiveSheet.Range("A2").Formula = "=A1*2"
  ActiveSheet.Columns("C").ColumnWidth = 30: ActiveSheet.Range("E5:F6").Merge
  ActiveSheet.Copy After:=ActiveSheet
  Probe = ActiveSheet.Name & "|idx " & ActiveSheet.Index & "|" & Tabs() & "|cw " & ActiveSheet.Columns("C").ColumnWidth & "|f " & ActiveSheet.Range("A2").Formula & "|merge " & ActiveSheet.Range("E5").MergeCells
 Case 21
  ActiveWorkbook.Sheets("Sheet1").Copy After:=ActiveWorkbook.Sheets("Sheet1")
  Probe = ActiveSheet.Name & "|" & Tabs()
 Case 22
  Set ws = ActiveWorkbook.Worksheets.Add: ws.Name = "ABCDEFGHIJKLMNOPQRSTUVWXYZ12345"
  ws.Copy After:=ws: Probe = ActiveSheet.Name & "|" & Len(ActiveSheet.Name)
  ActiveSheet.Copy After:=ActiveSheet: Probe = Probe & "|" & ActiveSheet.Name & "|" & Len(ActiveSheet.Name)
 Case 23
  Set ws = ActiveWorkbook.Worksheets.Add: ws.Name = "A": ws.Copy After:=ws: Probe = ActiveSheet.Name
  ActiveSheet.Copy After:=ActiveSheet: Probe = Probe & "|" & ActiveSheet.Name & "|" & Tabs()
 Case 24
  Set ws = ActiveWorkbook.Worksheets.Add: ws.Name = "A"
  ws.Copy Before:=ws: Probe = ActiveSheet.Name & "|" & ActiveSheet.Index & "|" & Tabs()
  ws.Copy Before:=ws: Probe = Probe & "||" & ActiveSheet.Name & "|" & ActiveSheet.Index & "|" & Tabs()
 Case 25
  Set ws = ActiveWorkbook.Worksheets.Add: ws.Name = "A": ws.Range("A1").Value = 1: ws.Copy
  Set b = ActiveWorkbook
  Probe = b.Name & "|" & Tabs() & "|" & Workbooks.Count
  b.Close False
 Case 26
  ActiveWorkbook.Worksheets.Add.Name = "P": ActiveWorkbook.Worksheets.Add.Name = "Q"
  Worksheets(Array("P", "Q")).Copy
  Set b = ActiveWorkbook
  Probe = b.Name & "|" & Tabs()
  b.Close False
 Case 27
  ActiveWorkbook.Worksheets.Add.Name = "P": ActiveWorkbook.Worksheets.Add.Name = "Q"
  Worksheets(Array("P", "Q")).Copy After:=Worksheets(Worksheets.Count)
  Probe = Tabs()
 Case 28
  ActiveWorkbook.Worksheets.Add.Name = "P": ActiveWorkbook.Worksheets.Add.Name = "Q"
  Worksheets(Array("P", "Q")).Copy Before:=Worksheets(1)
  Probe = Tabs()
 Case 30
  Set ws = ActiveSheet: ws.Name = "Sheet1": ws.Range("A1:A5").Value = 3
  ws.ChartObjects.Add(10, 10, 200, 100).Chart.SeriesCollection.NewSeries
  ws.ChartObjects(1).Chart.SeriesCollection(1).Formula = "=SERIES(,,Sheet1!$A$1:$A$5,1)"
  ws.Copy After:=ws
  Probe = ActiveSheet.Name & "|" & ActiveSheet.ChartObjects(1).Chart.SeriesCollection(1).Formula & "|src " & ws.ChartObjects(1).Chart.SeriesCollection(1).Formula
 Case 31
  Set ws = ActiveSheet: ws.Name = "Sheet1": ws.Range("A1:B3").Value = 1
  ws.Names.Add "Loc", "=Sheet1!$A$1"
  ws.ListObjects.Add(xlSrcRange, ws.Range("A1:B3"), , xlYes).Name = "T1"
  ws.Range("D1").AddComment "note"
  ws.Hyperlinks.Add ws.Range("D2"), "http://example.com"
  ws.Shapes.AddShape 1, 10, 100, 50, 50
  ws.Range("F1").Value = 1: ws.Range("F2").Value = 2
  ws.Range("G1").SparklineGroups.Add xlSparkLine, "Sheet1!F1:F2"
  ws.Copy After:=ws
  Set ws = ActiveSheet
  Probe = ws.Name & "|names " & ws.Names.Count & "|lo " & ws.ListObjects.Count & "|cm " & ws.Comments.Count & "|hl " & ws.Hyperlinks.Count & "|sh " & ws.Shapes.Count & "|sp " & ws.Range("G1").SparklineGroups.Count
  If ws.Names.Count > 0 Then Probe = Probe & "|nm " & ws.Names(1).Name & "=" & ws.Names(1).RefersTo
  If ws.ListObjects.Count > 0 Then Probe = Probe & "|loname " & ws.ListObjects(1).Name & " " & ws.ListObjects(1).Range.Address
 End Select
 Exit Function
Failed:
 Probe = Probe & " " & Er()
End Function

