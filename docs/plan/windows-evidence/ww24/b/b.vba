Private t As String
Private Sub Note(lbl As String)
 If Err.Number = 0 Then
  t = t & lbl & "=ok; "
 Else
  t = t & lbl & "=" & Err.Number & "; "
 End If
 Err.Clear
End Sub
Private Sub Setup()
 Dim ws As Object: Set ws = ActiveSheet
 On Error Resume Next
 ws.Unprotect
 ws.Cells.Clear
 ws.Cells.Locked = True
 ws.Range("A1:B1").Value = "h"
 ws.Range("A2").Value = 3: ws.Range("A3").Value = 1: ws.Range("A4").Value = 2
 ws.Range("B2").Value = "c": ws.Range("B3").Value = "a": ws.Range("B4").Value = "b"
 ws.Range("A1:B6").Locked = False
 ws.Range("D1:D6").Value = 5
 ws.Range("F1:F3").Value = 9
 On Error GoTo 0
End Sub
Private Sub Ops(ws As Object)
 On Error Resume Next
 ws.Range("D2").Font.Bold = True: Note "fmtLockedCell"
 ws.Range("A2").Font.Bold = True: Note "fmtUnlockedCell"
 ws.Columns("D").ColumnWidth = 15: Note "colWidth"
 ws.Rows(2).RowHeight = 30: Note "rowHeight"
 ws.Columns("H").Insert: Note "insCol"
 ws.Rows(8).Insert: Note "insRow"
 ws.Range("A5").Hyperlinks.Add ws.Range("A5"), "http://x.example": Note "hyperlink"
 ws.Columns("H").Delete: Note "delCol"
 ws.Rows(8).Delete: Note "delRow"
 ws.Range("A2:B4").Sort Key1:=ws.Range("A2"), Header:=xlNo: Note "sortUnlocked"
 ws.Range("D1:D6").Sort Key1:=ws.Range("D1"), Header:=xlNo: Note "sortLocked"
 ws.Range("A1").AutoFilter: Note "autofilterToggle"
 ws.Range("A1").Value = "z": Note "editUnlocked"
 ws.Range("D1").Value = "z": Note "editLocked"
End Sub
Public Function Probe(k As Long) As String
 Dim ws As Object, i As Long, lo As Object, arr As Variant, nm As String
 Set ws = ActiveSheet: t = ""
 On Error Resume Next
 Select Case k
 Case 1
  arr = Array("AllowFormattingCells", "AllowFormattingColumns", "AllowFormattingRows", "AllowInsertingColumns", "AllowInsertingRows", "AllowInsertingHyperlinks", "AllowDeletingColumns", "AllowDeletingRows", "AllowSorting", "AllowFiltering", "AllowUsingPivotTables", "none")
  For i = 0 To UBound(arr)
   Setup
   t = t & "[" & arr(i) & "] "
   Select Case i
   Case 0: ws.Protect , , , , , True
   Case 1: ws.Protect , , , , , , True
   Case 2: ws.Protect , , , , , , , True
   Case 3: ws.Protect , , , , , , , , True
   Case 4: ws.Protect , , , , , , , , , True
   Case 5: ws.Protect , , , , , , , , , , True
   Case 6: ws.Protect , , , , , , , , , , , True
   Case 7: ws.Protect , , , , , , , , , , , , True
   Case 8: ws.Protect , , , , , , , , , , , , , True
   Case 9: ws.Protect , , , , , , , , , , , , , , True
   Case 10: ws.Protect , , , , , , , , , , , , , , , True
   Case 11: ws.Protect
   End Select
   Note "protect"
   t = t & "readback fmtCells=" & ws.Protection.AllowFormattingCells & " fmtCols=" & ws.Protection.AllowFormattingColumns & " insCols=" & ws.Protection.AllowInsertingColumns & " sort=" & ws.Protection.AllowSorting & " filt=" & ws.Protection.AllowFiltering & " pivot=" & ws.Protection.AllowUsingPivotTables & " | "
   Ops ws
   t = t & vbLf
  Next
 Case 2
  Setup
  ws.Protect
  ws.Protect UserInterfaceOnly:=True: Note "reprotectUIO"
  t = t & "UIO-state: contents=" & ws.ProtectContents & " uio=" & ws.ProtectionMode & " | "
  ws.Range("D1").Value = "z": Note "editLocked"
  ws.Range("D1").Font.Bold = True: Note "fmtLocked"
  ws.Rows(8).Insert: Note "insRow"
  ws.Range("F1:F3").Cut ws.Range("G1"): Note "cutLocked"
  ws.Unprotect: Note "unprotect"
  ws.Protect Password:="pw"
  ws.Protect Password:="pw", UserInterfaceOnly:=True: Note "reprotectPwUIO"
  t = t & "after: contents=" & ws.ProtectContents & " uio=" & ws.ProtectionMode & " | "
  ws.Range("D2").Value = "q": Note "editLocked2"
  ws.Unprotect "pw": Note "unprotectpw"
  ws.Protect UserInterfaceOnly:=True: Note "protectUIOfresh"
  ws.Range("D2").Value = "q": Note "editLockedUIOfresh"
  t = t & "mode=" & ws.ProtectionMode
 Case 3
  Setup
  ws.Protect
  ws.Range("F1:F3").Cut ws.Range("A1"): Note "cutLockedOnProtected"
  ws.Range("A2").Cut ws.Range("A8"): Note "cutUnlockedToUnlocked"
  ws.Range("A3").Copy: ws.Range("D3").PasteSpecial: Note "pasteOverLocked"
  ws.Range("A3").Copy: ws.Range("A5").PasteSpecial: Note "pasteOverUnlocked"
 Case 4
  Setup
  ws.Protect
  Set lo = ws.ListObjects.Add(xlSrcRange, ws.Range("A1:B4"), , xlYes): Note "listAddUnlocked"
  Set lo = ws.ListObjects.Add(xlSrcRange, ws.Range("D1:D6"), , xlNo): Note "listAddLocked"
  ws.Unprotect
  Set lo = ws.ListObjects.Add(xlSrcRange, ws.Range("A1:B4"), , xlYes): Note "listAddUnprotected"
  ws.Range("D1:D3").Locked = False
  ws.Protect
  lo.Unlist: Note "unlist"
  t = t & "count=" & ws.ListObjects.Count & " "
  ws.Unprotect: Set lo = ws.ListObjects.Add(xlSrcRange, ws.Range("A1:B4"), , xlYes): ws.Protect
  lo.Delete: Note "listDelete"
  t = t & "count=" & ws.ListObjects.Count & " A2=" & ws.Range("A2").Value & " "
 Case 5
  Setup
  ws.Range("A1:B4").AutoFilter 1, "<>c"
  t = "filterMode=" & ws.FilterMode & " "
  ws.Protect
  ws.AutoFilterMode = False: Note "afModeFalse"
  ws.ShowAllData: Note "showAll"
  t = t & "filterMode=" & ws.FilterMode & " afMode=" & ws.AutoFilterMode & " | "
  ws.Unprotect
  Setup
  ws.Protect
  ws.Range("D11").Value = 1
  ws.Range("A11").Insert Shift:=xlDown: Note "insertCellUnlocked"
  ws.Range("A12").Delete Shift:=xlUp: Note "deleteCellUnlocked"
  ws.Range("A1").AutoFilter: Note "autofilterOneUnlockedCell"
  ws.Range("A1:B1").Copy: ws.Range("H1").PasteSpecial xlPasteColumnWidths: Note "pasteWidthsToLocked"
  ws.Range("A1:B1").Copy: ws.Range("B20").PasteSpecial xlPasteColumnWidths: Note "pasteWidthsToUnlocked"
 End Select
 Probe = t
End Function
