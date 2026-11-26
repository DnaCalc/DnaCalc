Private t As String
Private Sub Note(lbl As String)
 If Err.Number = 0 Then
  t = t & lbl & "=ok; "
 Else
  t = t & lbl & "=" & Err.Number & "|" & Err.Description & "; "
 End If
 Err.Clear
End Sub
Private Function V(x As Variant) As String
 If IsNull(x) Then V = "Null" Else V = CStr(x)
End Function
Private Function Sty(n As String) As String
 Dim s As Object, r As String
 On Error Resume Next
 Set s = ActiveWorkbook.Styles(n)
 r = n & ": "
 r = r & "ha=" & V(s.HorizontalAlignment): r = r & " va=" & V(s.VerticalAlignment): r = r & " ind=" & V(s.IndentLevel)
 r = r & " or=" & V(s.Orientation): r = r & " wrap=" & V(s.WrapText): r = r & " shrink=" & V(s.ShrinkToFit)
 r = r & " locked=" & V(s.Locked): r = r & " fh=" & V(s.FormulaHidden): r = r & " nf=" & V(s.NumberFormat)
 r = r & " val=" & V(s.Value): r = r & " bold=" & V(s.Font.Bold): r = r & " font=" & V(s.Font.Name) & "/" & V(s.Font.Size)
 r = r & " builtin=" & V(s.BuiltIn) & " | "
 Sty = r
End Function
Public Function Probe(k As Long) As String
 Dim ws As Object, i As Long, v As Variant, s As Object, c As Object
 Set ws = ActiveSheet: t = ""
 On Error Resume Next
 Select Case k
 Case 1
  v = Application.Match(9, Array(3, 5), 0): t = t & "match9 type=" & TypeName(v) & " | "
  v = Application.Match(5, Array(3, 5, 7), 0): t = t & "match5=" & v & " | "
  ws.Range("C1").Value = 1: ws.Range("C3").Value = "x"
  t = t & "countA=" & Application.CountA(ws.Range("C1:C3")) & " | "
  t = t & "transpose=" & Application.Transpose(Array(8, 9))(2, 1) & " | "
  Note "x"
  v = WorksheetFunction.Match(9, Array(3, 5), 0): Note "wsfMatch"
 Case 2
  Err.Clear: t = Sty("Normal"): t = t & Sty("Currency"): t = t & Sty("Percent"): t = t & Sty("Comma")
  Err.Clear: ActiveWorkbook.Styles.Add "Zed": Note "add"
  t = t & Sty("Zed")
 Case 3
  ws.Range("A1").Value = 1
  t = "color=" & ws.Range("A1").Interior.Color & " pc=" & ws.Range("A1").Interior.PatternColor & " pci=" & ws.Range("A1").Interior.PatternColorIndex & " | "
  Note "readPC"
  ws.Range("A1").Interior.PatternColor = 255: Note "putPCnofill"
  t = t & "pat=" & ws.Range("A1").Interior.Pattern & " pc=" & ws.Range("A1").Interior.PatternColor & " col=" & ws.Range("A1").Interior.Color & " | "
  ws.Range("A1").Interior.Color = 65280
  ws.Range("A1").Interior.PatternColor = 255: Note "putPCsolid"
  t = t & "pat=" & ws.Range("A1").Interior.Pattern & " pc=" & ws.Range("A1").Interior.PatternColor & " col=" & ws.Range("A1").Interior.Color & " | "
  ws.Range("A2").Interior.Pattern = xlPatternLinearGradient: Note "linGrad"
  t = t & "pat=" & ws.Range("A2").Interior.Pattern & " | "
  ws.Range("A3").Interior.Color = 255: ws.Range("A3").Interior.Pattern = xlPatternAutomatic: Note "autoPat"
  t = t & "pat=" & ws.Range("A3").Interior.Pattern & " col=" & ws.Range("A3").Interior.Color & " | "
  ws.Range("A4").Interior.Pattern = xlPatternGray50: ws.Range("A4").Interior.PatternColor = 255: ws.Range("A4").Interior.Color = 65280: Note "gray50"
  t = t & "pat=" & ws.Range("A4").Interior.Pattern & " pc=" & ws.Range("A4").Interior.PatternColor & " col=" & ws.Range("A4").Interior.Color & " ci=" & ws.Range("A4").Interior.ColorIndex & " tint=" & ws.Range("A4").Interior.TintAndShade
 Case 4
  ws.Range("B2").Value = "x": ws.Range("B2").Font.Bold = True: ws.Range("B2").Interior.Color = 255: ws.Range("B2").NumberFormat = "0.00"
  Dim rr As Range: Set rr = ws.Range("B2"): rr.Style = "Hyperlink": Note "style"
  t = t & "stylesCount=" & ActiveWorkbook.Styles.Count & " ": Note "cnt"
  rr.Style = ActiveWorkbook.Styles("Hyperlink"): Note "style2"
  t = t & "bold=" & ws.Range("B2").Font.Bold & " fill=" & ws.Range("B2").Interior.Color & " nf=" & ws.Range("B2").NumberFormat & " ul=" & ws.Range("B2").Font.Underline & " col=" & ws.Range("B2").Font.Color & " style=" & ws.Range("B2").Style & " | "
  ws.Range("B2").Font.Bold = True: t = t & "afterDirect style=" & ws.Range("B2").Style & " | "
  ActiveWorkbook.SaveAs Environ("TEMP") & "\ww24c4.xlsx", 51: Note "save"
 Case 5
  ws.Range("A1:B2").Select
  ws.Range("D4:E5").Activate: Note "activate"
  t = "sel=" & Selection.Address & " active=" & ActiveCell.Address & " | "
  ws.Range("A1:B2").Select: ws.Range("B2:C3").Activate: Note "act2"
  t = t & "sel=" & Selection.Address & " active=" & ActiveCell.Address & " | "
  ws.Range("A1:B2").Select: ws.Range("B2").Activate
  t = t & "sel=" & Selection.Address & " active=" & ActiveCell.Address & " | "
  ws.Range("A1:B2").Select: ws.Range("A2:B3").Activate
  t = t & "sel=" & Selection.Address & " active=" & ActiveCell.Address
 Case 6
  ActiveWorkbook.Worksheets.Add(After:=ws).Name = "S2": ActiveWorkbook.Worksheets.Add(After:=ActiveWorkbook.Worksheets("S2")).Name = "S3"
  ActiveWorkbook.Worksheets("S2").Activate
  ActiveWorkbook.Worksheets("S3").Select Replace:=False: Note "selGroup"
  t = "active=" & ActiveSheet.Name & " selected=" & ActiveWindow.SelectedSheets.Count & " :"
  For Each s In ActiveWindow.SelectedSheets: t = t & s.Name & ",": Next
  ActiveCell.Value = 7: Note "putActive"
  t = t & " | S2.A1=" & ActiveWorkbook.Worksheets("S2").Range("A1").Value & " S3.A1=" & ActiveWorkbook.Worksheets("S3").Range("A1").Value
  Selection.Value2 = 9
  t = t & " | after Selection.Value2: S2.A1=" & ActiveWorkbook.Worksheets("S2").Range("A1").Value & " S3.A1=" & ActiveWorkbook.Worksheets("S3").Range("A1").Value
  ActiveWorkbook.Worksheets("S2").Range("B2").Value = 5
  t = t & " | S3.B2=" & ActiveWorkbook.Worksheets("S3").Range("B2").Value
  ActiveWorkbook.Worksheets("S2").Range("C1:C2").Value = 1
  t = t & " S3.C1=" & ActiveWorkbook.Worksheets("S3").Range("C1").Value & " | sel type=" & TypeName(Selection)
  ActiveWorkbook.Worksheets("S3").Select Replace:=True
  t = t & " | after Replace:True count=" & ActiveWindow.SelectedSheets.Count
 Case 7
  ActiveWorkbook.Charts.Add
  t = "sheet=" & TypeName(ActiveSheet) & " "
  t = t & "selType=" & TypeName(Selection) & " "
  Note "sel"
  t = t & "activeCell: "
  v = ActiveCell.Address: Note "activeCell"
  t = t & " activeWindowType=" & ActiveWindow.Type
 Case 8
  Set s = ActiveWorkbook.Worksheets.Add
  s.Shapes("nosuch").Delete: Note "shapeDelete"
  Application.Toolbars("Standard").Visible = True: Note "toolbarVisible"
  Application.CommandBars("Cell").Enabled = True: Note "cbCell"
  t = t & "dialogs=" & Application.Dialogs.Count & " "
  Note "dialogsCount"
  v = Application.Toolbars.Count: Note "toolbarsCount": t = t & "toolbars=" & v & " "
  v = Application.CommandBars("Standard").Visible: Note "cbStandardVisible": t = t & " v=" & v
 End Select
 Probe = t
End Function



