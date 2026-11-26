Private t As String
Private Sub Note(lbl As String)
 If Err.Number = 0 Then
  t = t & lbl & "=ok; "
 Else
  t = t & lbl & "=" & Err.Number & "|" & Err.Description & "; "
 End If
 Err.Clear
End Sub
Private Function Mk(ws As Object, nm As String, l As Double, tp As Double) As Object
 Dim co As Object
 Set co = ws.ChartObjects.Add(l, tp, 300, 200)
 co.Chart.ChartType = xlColumnClustered
 co.Chart.SetSourceData ws.Range("A1:B4")
 Set Mk = co
End Function
Private Sub Fill(ws As Object)
 ws.Range("A1:B1").Value = Array("cat", "val")
 ws.Range("A2:B2").Value = Array("x", 1)
 ws.Range("A3:B3").Value = Array("y", 2)
 ws.Range("A4:B4").Value = Array("z", 3)
End Sub
Private Function V(x As Variant) As String
 If IsNull(x) Then
  V = "Null"
 ElseIf IsEmpty(x) Then
  V = "Empty"
 Else
  V = CStr(x)
 End If
End Function
Private Sub SaveIt(k As Long)
 ActiveWorkbook.SaveAs Environ("TEMP") & "\ww24f" & k & ".xlsx", 51
End Sub
Public Function Probe(k As Long) As String
 Dim ws As Object, co As Object, co2 As Object, s As Object, p As Object, i As Long, x As Variant, sh As Object
 Set ws = ActiveSheet: t = "": Fill ws
 On Error Resume Next
 Select Case k
 Case 1
  Set co = Mk(ws, "c1", 10, 10): Set s = co.Chart.SeriesCollection(1)
  t = t & "pointsType=" & TypeName(s.Points) & " count=" & s.Points.Count & " | ": Note "pts"
  For Each x In Array(0, 1, 3, 4, "a", -1)
   Set p = Nothing: Set p = s.Points(x): Note "Points(" & x & ")"
   If Not p Is Nothing Then t = t & "type=" & TypeName(p) & " "
  Next
  Set p = s.Points: Set p = s.Points(): Note "Points()"
  Set p = s.Points.Item(2): Note "Item(2)": t = t & "item2=" & TypeName(p) & " name=" & V(p.Name) & " | "
  ws.Range("B3").ClearContents
  t = t & "blank: count=" & s.Points.Count & " "
  Set p = s.Points(2): Note "blankPt2"
  ws.Range("A1:B4").ClearContents
  t = t & "empty: count=" & s.Points.Count & " "
  Set p = s.Points(1): Note "emptyPt1"
 Case 2
  Set co = Mk(ws, "c2", 10, 10): Set s = co.Chart.SeriesCollection(1)
  t = "series.HasDataLabels=" & V(s.HasDataLabels) & " "
  Set p = s.Points(1)
  t = t & "p1.HasDataLabel=" & V(p.HasDataLabel) & " ": Note "p1has"
  x = p.DataLabel.Text: Note "p1.DataLabel.Text(off)"
  p.HasDataLabel = True: Note "p1on"
  t = t & "afterOn: p1=" & V(s.Points(1).HasDataLabel) & " p2=" & V(s.Points(2).HasDataLabel) & " p3=" & V(s.Points(3).HasDataLabel) & " series.Has=" & V(s.HasDataLabels) & " | "
  t = t & "dl.Count=" & V(s.DataLabels.Count) & " ": Note "dlCount"
  t = t & "p1.text=" & V(s.Points(1).DataLabel.Text) & " ": Note "p1text"
  SaveIt 2
  s.HasDataLabels = True
  s.Points(2).HasDataLabel = False: Note "p2off"
  t = t & "| series on, p2 off: has=" & V(s.HasDataLabels) & " p1=" & V(s.Points(1).HasDataLabel) & " p2=" & V(s.Points(2).HasDataLabel) & " p3=" & V(s.Points(3).HasDataLabel) & " dl.Count=" & V(s.DataLabels.Count) & " "
  x = s.DataLabels(2).Text: Note "DataLabels(2)"
  x = s.Points(2).DataLabel.Text: Note "p2.DataLabel.Text"
  ActiveWorkbook.SaveAs Environ("TEMP") & "\ww24f2b.xlsx", 51
 Case 3
  Set co = Mk(ws, "c3", 10, 10): Set s = co.Chart.SeriesCollection(1)
  s.HasDataLabels = True
  t = "default size=" & V(s.DataLabels.Font.Size) & " bold=" & V(s.DataLabels.Font.Bold) & " color=" & V(s.DataLabels.Font.Color) & " | "
  s.DataLabels.Font.Size = 14: s.DataLabels.Font.Bold = True: s.DataLabels.Font.Color = 255: Note "put"
  t = t & "p1: size=" & V(s.Points(1).DataLabel.Font.Size) & " bold=" & V(s.Points(1).DataLabel.Font.Bold) & " color=" & V(s.Points(1).DataLabel.Font.Color) & " | "
  s.Points(2).DataLabel.Font.Size = 9: Note "p2size"
  t = t & "series size=" & V(s.DataLabels.Font.Size) & " p2=" & V(s.Points(2).DataLabel.Font.Size) & " p1=" & V(s.Points(1).DataLabel.Font.Size) & " | "
  SaveIt 3
 Case 4
  Set co = Mk(ws, "c4", 10, 10): Set s = co.Chart.SeriesCollection(1)
  s.HasDataLabels = True
  Set p = s.Points(2)
  t = "nf=" & V(p.DataLabel.NumberFormat) & " linked=" & V(p.DataLabel.NumberFormatLinked) & " text=" & V(p.DataLabel.Text) & " caption=" & V(p.DataLabel.Caption) & " left=" & V(p.DataLabel.Left) & " top=" & V(p.DataLabel.Top) & " | "
  p.DataLabel.NumberFormat = "0.00": Note "nf"
  t = t & "after nf=0.00: nf=" & V(p.DataLabel.NumberFormat) & " linked=" & V(p.DataLabel.NumberFormatLinked) & " text=" & V(p.DataLabel.Text) & " p1nf=" & V(s.Points(1).DataLabel.NumberFormat) & " p1linked=" & V(s.Points(1).DataLabel.NumberFormatLinked) & " | "
  p.DataLabel.NumberFormatLinked = True: Note "linkTrue"
  t = t & "linkTrue: nf=" & V(p.DataLabel.NumberFormat) & " linked=" & V(p.DataLabel.NumberFormatLinked) & " | "
  p.DataLabel.NumberFormat = "General": Note "nfGeneral"
  t = t & "nfGeneral: linked=" & V(p.DataLabel.NumberFormatLinked) & " | "
  p.DataLabel.NumberFormatLinked = False: Note "linkFalse"
  t = t & "linkFalse: nf=" & V(p.DataLabel.NumberFormat) & " linked=" & V(p.DataLabel.NumberFormatLinked) & " | "
  p.DataLabel.Text = "hello": Note "text"
  t = t & "text: " & V(p.DataLabel.Text) & " caption=" & V(p.DataLabel.Caption) & " | "
  ws.Range("B3").Value = 20
  t = t & "after source edit text=" & V(p.DataLabel.Text) & " | "
  p.DataLabel.Left = 50.5: p.DataLabel.Top = 20.25: Note "pos"
  t = t & "left=" & V(p.DataLabel.Left) & " top=" & V(p.DataLabel.Top)
  SaveIt 4
 Case 5
  Set co = Mk(ws, "c5", 10, 10): Set co2 = Mk(ws, "c5b", 330, 10)
  Set sh = ws.Shapes.AddShape(1, 10, 250, 50, 50)
  t = "names: " & co.Name & "," & co2.Name & "," & sh.Name & " chart.Name=" & V(co.Chart.Name) & " | "
  co.Name = "A&B<C>""'": Note "nameMeta"
  t = t & "co=" & co.Name & " chart.Name=" & V(co.Chart.Name) & " | "
  co.Name = co2.Name: Note "nameDup"
  t = t & "co=" & co.Name & " co2=" & co2.Name & " | "
  co.Name = sh.Name: Note "nameDupShape"
  t = t & "co=" & co.Name & " | "
  co.Name = "Alpha"
  co.Chart.Name = "Beta": Note "chartName"
  t = t & "after chart.Name: co=" & co.Name & " chart=" & V(co.Chart.Name) & " lookup Alpha=" & TypeName(ws.ChartObjects("Alpha")) & " | "
  Err.Clear
  x = TypeName(ws.ChartObjects("Beta")): Note "lookupBeta": t = t & x & " | "
  Set co2 = Nothing
  co.Left = 50.7: co.Top = 33.3: co.Width = 211.9: co.Height = 123.4: Note "geom"
  t = t & "L/T/W/H=" & co.Left & "," & co.Top & "," & co.Width & "," & co.Height & " | id=" & V(co.ShapeRange.ID) & " placement=" & V(co.Placement)
  SaveIt 5
 Case 6
  Set co = Mk(ws, "c6", 10, 10)
  ws.Rows(1).RowHeight = 40: ws.Columns("A").ColumnWidth = 20
  co.Left = ws.Range("C3").Left + 3.3: co.Top = ws.Range("C3").Top + 1.7
  t = "L/T=" & co.Left & "," & co.Top & " tl=" & co.TopLeftCell.Address & " br=" & co.BottomRightCell.Address & " placement=" & co.Placement & " | "
  co.Placement = xlMove: co.Left = 100: Note "move"
  t = t & "xlMove: tl=" & co.TopLeftCell.Address & " br=" & co.BottomRightCell.Address & " | "
  co.Placement = xlFreeFloating: co.Left = 120
  t = t & "free: tl=" & co.TopLeftCell.Address & " br=" & co.BottomRightCell.Address & " | "
  SaveIt 6
 End Select
 Probe = t
End Function
