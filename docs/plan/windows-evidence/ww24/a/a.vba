Private Function Er() As String
 Er = "ERR " & Err.Number & "|" & Err.Description
End Function
Private Function Tabs() As String
 Dim s As Object, t As String
 For Each s In ActiveWorkbook.Sheets: t = t & s.Name & ",": Next
 Tabs = t
End Function
Private Function Cf(r As Range) As String
 Dim i As Long, t As String, f As Object
 t = "n=" & r.FormatConditions.Count
 For i = 1 To r.FormatConditions.Count
  Set f = r.FormatConditions(i)
  t = t & " [" & i & " type=" & f.Type & " f1=" & f.Formula1 & " at=" & f.AppliesTo.Address & " pri=" & f.Priority & "]"
 Next
 Cf = t
End Function
Private Function W(c As String) As String
 With ActiveSheet.Columns(c)
  W = c & ":w=" & .ColumnWidth & ",h=" & .Hidden & ",o=" & .OutlineLevel & " "
 End With
End Function
Public Function Probe(k As Long) As String
 Dim ws As Object, s As Object, t As String, i As Long, b2 As Workbook, lo As Object
 On Error GoTo Failed
 Set ws = ActiveSheet
 Select Case k
 Case 1
  Probe = "xlPasteCommentsAndNotes=" & CLng(xlPasteCommentsAndNotes)
 Case 2, 3
  ws.Columns("A").ColumnWidth = 20: ws.Columns("B").ColumnWidth = 5
  ws.Columns("A").OutlineLevel = 1: ws.Columns("B").Hidden = True
  ws.Range("A1:B1").Copy
  If k = 2 Then ws.Range("D1").PasteSpecial Paste:=xlPasteColumnWidths Else ws.Range("D1:D3").PasteSpecial Paste:=xlPasteColumnWidths
  Probe = W("D") & W("E") & W("F") & W("G") & " sel=" & Selection.Address
 Case 4
  ws.Range("A1:A3").Value = 7: ws.Range("C1:C3").Value = 12
  ws.Range("A1:A3").FormatConditions.Add(xlCellValue, xlGreater, "=5").Interior.Color = 255
  ws.Range("C1:C3").FormatConditions.Add(xlCellValue, xlGreater, "=10").Interior.Color = 65280
  t = "before dest " & Cf(ws.Range("A1:A3")) & " src " & Cf(ws.Range("C1:C3"))
  ws.Range("C1:C3").Copy
  ws.Range("A1:A3").PasteSpecial Paste:=14
  Probe = t & " | after dest " & Cf(ws.Range("A1:A3"))
 Case 5
  ws.Range("A1").Value = 5: ws.Range("C1").Value = 9
  For i = 1 To 4
   ws.Range("E1:F1").ClearContents: ws.Range("F1").Value = 9
   ws.Range("D1").Copy
   Select Case i
   Case 1: ws.Range("E1:F1").PasteSpecial Paste:=xlPasteAll, Operation:=xlAdd
   Case 2: ws.Range("E1:F1").PasteSpecial Paste:=xlPasteAll, Operation:=xlSubtract
   Case 3: ws.Range("E1:F1").PasteSpecial Paste:=xlPasteAll, Operation:=xlMultiply
   Case 4: ws.Range("E1:F1").PasteSpecial Paste:=xlPasteAll, Operation:=xlDivide
   End Select
   t = t & "op" & i & " blankdest=[" & ws.Range("E1").Text & "|" & TypeName(ws.Range("E1").Value2) & "] numdest=[" & ws.Range("F1").Text & "|" & TypeName(ws.Range("F1").Value2) & "] "
  Next
  Probe = t
 Case 6
  ws.Range("A1").Value = 1234.5: ws.Range("A1").NumberFormat = "0.000": ws.Range("A1").Font.Bold = True
  ws.Range("A1").Interior.Color = 255: ws.Range("A1").HorizontalAlignment = xlRight
  ws.Range("A1").Borders(xlEdgeBottom).LineStyle = xlContinuous: ws.Range("A1").Locked = False
  ws.Range("B1").Value = "'abc": ws.Range("B1").NumberFormat = "@"
  ws.Range("C1").Value = 5: ws.Range("C1").NumberFormat = "0%": ws.Range("C1").HorizontalAlignment = xlCenter: ws.Range("C1").Font.Italic = True
  ws.Range("C1").Interior.Color = 65280: ws.Range("C1").Borders(xlEdgeTop).LineStyle = xlContinuous: ws.Range("C1").Locked = True
  ws.Range("A1").Copy
  ws.Range("C1").PasteSpecial Paste:=7
  t = "7: nf=" & ws.Range("C1").NumberFormat & " b=" & ws.Range("C1").Font.Bold & " i=" & ws.Range("C1").Font.Italic & " fill=" & ws.Range("C1").Interior.Color & " al=" & ws.Range("C1").HorizontalAlignment & " top=" & ws.Range("C1").Borders(xlEdgeTop).LineStyle & " bot=" & ws.Range("C1").Borders(xlEdgeBottom).LineStyle & " lock=" & ws.Range("C1").Locked & " v=" & ws.Range("C1").Value2 & " | "
  ws.Range("C1").Value = 5: ws.Range("C1").NumberFormat = "0%": ws.Range("C1").HorizontalAlignment = xlCenter: ws.Range("C1").Font.Italic = True: ws.Range("C1").Interior.Color = 65280: ws.Range("C1").Locked = True
  ws.Range("A1").Copy: ws.Range("C1").PasteSpecial Paste:=11
  t = t & "11: nf=" & ws.Range("C1").NumberFormat & " b=" & ws.Range("C1").Font.Bold & " fill=" & ws.Range("C1").Interior.Color & " al=" & ws.Range("C1").HorizontalAlignment & " lock=" & ws.Range("C1").Locked & " | "
  ws.Range("C1").Value = 5: ws.Range("C1").NumberFormat = "0%": ws.Range("C1").HorizontalAlignment = xlCenter: ws.Range("C1").Interior.Color = 65280
  ws.Range("A1").Copy: ws.Range("C1").PasteSpecial Paste:=12
  t = t & "12: nf=" & ws.Range("C1").NumberFormat & " b=" & ws.Range("C1").Font.Bold & " fill=" & ws.Range("C1").Interior.Color & " al=" & ws.Range("C1").HorizontalAlignment & " lock=" & ws.Range("C1").Locked & " v=" & ws.Range("C1").Value2 & " | "
  ws.Range("B1").Copy: ws.Range("D1").PasteSpecial Paste:=7
  t = t & "7text: qp=" & ws.Range("D1").PrefixCharacter & " v=" & ws.Range("D1").Value2 & " nf=" & ws.Range("D1").NumberFormat
  Probe = t
 Case 7
  ws.Range("A1").Value = 1: ws.Range("B1").Value = 2: ws.Range("C1").Value = 3
  ws.Range("A1").Validation.Add Type:=xlValidateWholeNumber, AlertStyle:=xlValidAlertStop, Operator:=xlBetween, Formula1:="1", Formula2:="10"
  ws.Range("B1").Validation.Add Type:=xlValidateDecimal, Operator:=xlGreater, Formula1:="0"
  ws.Range("A1").Copy: ws.Range("B1").PasteSpecial Paste:=6
  t = "val onto own: type=" & ws.Range("B1").Validation.Type & " f1=" & ws.Range("B1").Validation.Formula1 & " | "
  ws.Range("C1").AddComment "srcnote"
  ws.Range("D1").AddComment "destnote"
  ws.Range("C1").Copy: ws.Range("D1").PasteSpecial Paste:=-4144
  t = t & "notes onto note: " & ws.Range("D1").Comment.Text & " | "
  ws.Range("E1").AddComment "keep"
  ws.Range("F1").Copy: ws.Range("E1").PasteSpecial Paste:=-4144
  t = t & "blank src onto note: "
  If ws.Range("E1").Comment Is Nothing Then t = t & "none" Else t = t & ws.Range("E1").Comment.Text
  Probe = t
 Case 8
  ws.Name = "S1"
  Set b2 = Workbooks.Add: b2.Worksheets(1).Name = "T1"
  ws.Parent.Activate
  ws.Copy Before:=b2.Worksheets(1)
  Probe = "copy: wb2 tabs=" & TabsOf(b2) & " wb1 tabs=" & TabsOf(ws.Parent) & " activebook=" & ActiveWorkbook.Name & " activesheet=" & ActiveSheet.Name & " count=" & Workbooks.Count
 Case 9
  Dim wbA As Workbook
  Set wbA = ws.Parent
  wbA.Worksheets(1).Name = "S1": wbA.Worksheets.Add(After:=wbA.Worksheets(1)).Name = "S2"
  Set b2 = Workbooks.Add: b2.Worksheets(1).Name = "T1"
  t = "A=" & wbA.Name & " B=" & b2.Name & " "
  wbA.Worksheets("S1").Move Before:=b2.Worksheets("T1")
  Probe = t & "move: B tabs=" & TabsOf(b2) & " A tabs=" & TabsOf(wbA) & " active=" & ActiveWorkbook.Name & "/" & ActiveSheet.Name
 Case 14
  ws.Columns("A").ColumnWidth = 20: ws.Columns("B").ColumnWidth = 5
  ws.Columns("A").OutlineLevel = 1: ws.Columns("B").Hidden = True
  ws.Range("A1:B1").Copy
  ws.Range("D1:G1").PasteSpecial Paste:=xlPasteColumnWidths
  Probe = W("D") & W("E") & W("F") & W("G") & " sel=" & Selection.Address
 Case 15
  ws.Range("A1").Value = 3: ws.Range("A1").AddComment "n1"
  ws.Range("A1").Copy
  ws.Range("C1").PasteSpecial Paste:=21
  If ws.Range("C1").Comment Is Nothing Then Probe = "21 no note" Else Probe = "21 note " & ws.Range("C1").Comment.Text
 Case 16
  ws.Columns("A").ColumnWidth = 20: ws.Columns("B").ColumnWidth = 30
  ws.Range("A1").Value = "x": ws.Range("A2").Value = "y"
  ws.Range("A1:B2").Copy
  ws.Range("E5").PasteSpecial Paste:=xlPasteColumnWidths, Transpose:=True
  t = "tr: " & W("E") & W("F") & W("G") & "e5=[" & ws.Range("E5").Value2 & "] sel=" & Selection.Address & " | "
  ws.Range("A1:B2").Copy
  ws.Range("I5").PasteSpecial Paste:=xlPasteColumnWidths, Operation:=xlAdd
  t = t & "op: " & W("I") & W("J") & "i5=[" & ws.Range("I5").Value2 & "] | "
  ws.Range("A1:B2").Copy
  ws.Range("L5").PasteSpecial Paste:=13
  t = t & "13: " & W("L") & W("M") & "l5=" & ws.Range("L5").Value2 & " | "
  ws.Range("A1:B2").Copy
  ws.Range("O5").PasteSpecial Paste:=14
  Probe = t & "14: " & W("O") & W("P") & "o5=" & ws.Range("O5").Value2
 Case 10, 11, 12, 13
  ws.Name = "S1": ws.Parent.Worksheets.Add(After:=ws).Name = "S2": ws.Parent.Worksheets.Add(After:=ws.Parent.Worksheets("S2")).Name = "S3": ws.Parent.Worksheets.Add(After:=ws.Parent.Worksheets("S3")).Name = "S4"
  Select Case k
  Case 10: ws.Parent.Worksheets(Array(2, 3)).Move Before:=ws.Parent.Worksheets(2)
  Case 11: ws.Parent.Worksheets(Array(2, 3)).Move After:=ws.Parent.Worksheets(3)
  Case 12: ws.Parent.Worksheets(Array(3, 2)).Move Before:=ws.Parent.Worksheets(1)
  Case 13: ws.Parent.Worksheets(Array(4, 2)).Move After:=ws.Parent.Worksheets(1)
  End Select
  Probe = "tabs=" & Tabs() & " active=" & ActiveSheet.Name
 End Select
 Exit Function
Failed:
 Probe = Er()
End Function
Private Function TabsOf(b As Workbook) As String
 Dim s As Object, t As String
 For Each s In b.Sheets: t = t & s.Name & ",": Next
 TabsOf = t
End Function


