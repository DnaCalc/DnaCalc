Private t As String
Private Sub Note(lbl As String)
 If Err.Number = 0 Then
  t = t & lbl & "=ok; "
 Else
  t = t & lbl & "=" & Err.Number & "|" & Err.Description & "; "
 End If
 Err.Clear
End Sub
Private Sub Data(ws As Object)
 ws.Range("A1:C1").Value = Array("Item", "Qty", "Region")
 ws.Range("A2:C2").Value = Array("a", 10, "N")
 ws.Range("A3:C3").Value = Array("b", 20, "S")
 ws.Range("A4:C4").Value = Array("a", 10, "S")
 ws.Range("A5:C5").Value = Array("c", 30, "N")
 ws.Range("A6:C6").Value = Array("a", 10, "N")
 ws.Range("E1").Value = "Qty": ws.Range("E2").Value = ">=10"
End Sub
Private Function Rg(ws As Object, a As String) As String
 Dim c As Object, r As String
 For Each c In ws.Range(a).Cells: r = r & c.Text & ",": Next
 Rg = r
End Function
Public Function Probe(k As Long) As String
 Dim ws As Object, v As Variant, i As Long
 Set ws = ActiveSheet: t = ""
 On Error Resume Next
 Select Case k
 Case 1
  Data ws
  v = ws.Range("A1:C6").AdvancedFilter(xlFilterInPlace, ws.Range("E1:E2")): Note "inplace": t = t & "ret=" & v & " "
  ws.Range("E2").Value = ">=20"
  v = ws.Range("A1:C6").AdvancedFilter(xlFilterInPlace, ws.Range("E1:E2")): Note "inplace2"
  t = t & "hidden:"
  For i = 1 To 6: t = t & ws.Rows(i).Hidden & ",": Next
  t = t & " filterMode=" & ws.FilterMode & " afMode=" & ws.AutoFilterMode & " |"
  ActiveWorkbook.SaveAs Environ("TEMP") & "\ww24e1.xlsx", 51: Note "save"
  Dim wb As Workbook: Set wb = ActiveWorkbook
  wb.Close False: Note "close"
  Set wb = Workbooks.Open(Environ("TEMP") & "\ww24e1.xlsx"): Note "reopen"
  Set ws = wb.Worksheets(1)
  t = t & " reopen hidden:"
  For i = 1 To 6: t = t & ws.Rows(i).Hidden & ",": Next
  t = t & " filterMode=" & ws.FilterMode & " afMode=" & ws.AutoFilterMode
  ws.ShowAllData: Note "showAll"
  t = t & " afterShowAll hidden:"
  For i = 1 To 6: t = t & ws.Rows(i).Hidden & ",": Next
  t = t & " filterMode=" & ws.FilterMode & " names="
  For i = 1 To wb.Names.Count: t = t & wb.Names(i).Name & "=" & wb.Names(i).RefersTo & ";": Next
  wb.Close False
 Case 2
  Data ws
  ws.Range("E1:E2").ClearContents
  ws.Range("A1:C6").AdvancedFilter xlFilterCopy, , ws.Range("G1:H1"), True: Note "uniqAB-hdrItemRegion"
  ws.Range("G1:H1").Value = Array("Item", "Qty")
  ws.Range("G2:H20").ClearContents
  ws.Range("A1:C6").AdvancedFilter xlFilterCopy, , ws.Range("G1:H1"), True: Note "uniqItemQty"
  t = t & "G1:H6=" & Rg(ws, "G1:H6") & " | "
  ws.Range("J1:K10").ClearContents
  ws.Range("J1:K1").Value = Array("Item", "Region")
  ws.Range("A1:C6").AdvancedFilter xlFilterCopy, , ws.Range("J1:K1"), True: Note "uniqItemRegion"
  t = t & "J1:K6=" & Rg(ws, "J1:K6") & " | "
  ws.Range("M1:M10").ClearContents: ws.Range("M1").Value = "Item"
  ws.Range("A1:C6").AdvancedFilter xlFilterCopy, , ws.Range("M1"), True: Note "uniqItem"
  t = t & "M1:M6=" & Rg(ws, "M1:M6")
 Case 3
  Data ws
  ws.Range("E1:E2").ClearContents
  ws.Range("G1").Value = "Item": ws.Range("I1").Value = "Region"
  ws.Range("A1:C6").AdvancedFilter xlFilterCopy, , ws.Range("G1:I1"): Note "blankMiddle"
  t = t & "G1:J3=" & Rg(ws, "G1:J3") & " | "
  ws.Range("L1").Value = "": ws.Range("M1").Value = "Qty"
  ws.Range("A1:C6").AdvancedFilter xlFilterCopy, , ws.Range("L1:M1"): Note "blankFirst"
  t = t & "L1:N3=" & Rg(ws, "L1:N3") & " | "
  ws.Range("P1").Value = "Item": ws.Range("Q1").Value = "Item"
  ws.Range("A1:C6").AdvancedFilter xlFilterCopy, , ws.Range("P1:Q1"): Note "dupHeader"
  t = t & "P1:Q3=" & Rg(ws, "P1:Q3")
 End Select
 Probe = t
End Function
