Private t As String
Private Sub Note(lbl As String)
 If Err.Number = 0 Then
  t = t & lbl & "=ok; "
 Else
  t = t & lbl & "=" & Err.Number & "|" & Err.Description & "; "
 End If
 Err.Clear
End Sub
Private Function St(ws As Object) As String
 St = "[c=" & ws.ProtectContents & ",d=" & ws.ProtectDrawingObjects & ",s=" & ws.ProtectScenarios & ",m=" & ws.ProtectionMode & "]"
End Function
Private Sub Prot(ws As Object, kind As Long)
 Dim e As Variant
 ws.Unprotect "abc": ws.Unprotect "": ws.Unprotect 123: ws.Unprotect 1.5
 Err.Clear
 Select Case kind
 Case 1: ws.Protect
 Case 2: ws.Protect Password:=e
 Case 3: ws.Protect Password:=""
 Case 4: ws.Protect Password:=123
 Case 5: ws.Protect Password:=1.5
 Case 6: ws.Protect Password:="abc"
 End Select
End Sub
Public Function Probe(k As Long) As String
 Dim ws As Object, e As Variant, i As Long, tries As Variant, a As Variant
 Set ws = ActiveSheet: t = ""
 On Error Resume Next
 Select Case k
 Case 1, 2, 3, 4, 5
  Prot ws, CLng(k)
  Note "protect": t = t & St(ws) & " | "
  ws.Unprotect "x": Note "unprotect(x)": t = t & St(ws) & " | "
  If ws.ProtectContents Then ws.Unprotect "": Note "unprotect("""")": t = t & St(ws) & " | "
  If ws.ProtectContents Then ws.Unprotect "123": Note "unprotect(""123"")": t = t & St(ws) & " | "
  If ws.ProtectContents Then ws.Unprotect 123: Note "unprotect(123)": t = t & St(ws) & " | "
  If ws.ProtectContents Then ws.Unprotect "1.5": Note "unprotect(""1.5"")": t = t & St(ws) & " | "
  If ws.ProtectContents Then ws.Unprotect 1.5: Note "unprotect(1.5)": t = t & St(ws) & " | "
  If ws.ProtectContents Then ws.Unprotect e: Note "unprotect(Empty)": t = t & St(ws) & " | "
  If ws.ProtectContents Then ws.Unprotect: Note "unprotect()": t = t & St(ws)
 Case 6
  Prot ws, 6
  ws.Unprotect: Note "unprotect-omitted": t = t & St(ws)
 Case 7
  Prot ws, 6
  ws.Unprotect e: Note "unprotect-Empty": t = t & St(ws)
 Case 8
  Prot ws, 1
  ws.Unprotect: Note "unprotect-omitted-nopw": t = t & St(ws)
 Case 9
  Prot ws, 6
  ws.Protect Password:="other": Note "protectTwiceOtherPw": t = t & St(ws) & " "
  ws.Unprotect "abc": Note "unprotect(abc)": t = t & St(ws) & " "
  ws.Unprotect "other": Note "unprotect(other)": t = t & St(ws) & " "
  Prot ws, 6
  ws.Protect DrawingObjects:=False, Contents:=True, Scenarios:=False: Note "protectTwiceNoPw": t = t & St(ws) & " "
  ws.Unprotect "abc": Note "unprotect(abc)again": t = t & St(ws)
 Case 10
  Dim w1 As Object, w2 As Object, w3 As Object, w4 As Object, w5 As Object
  Set w1 = ActiveWorkbook.Worksheets(1)
  Set w2 = ActiveWorkbook.Worksheets.Add(After:=w1): Set w3 = ActiveWorkbook.Worksheets.Add(After:=w2): Set w4 = ActiveWorkbook.Worksheets.Add(After:=w3): Set w5 = ActiveWorkbook.Worksheets.Add(After:=w4)
  w1.Protect
  w2.Protect "abc"
  w3.Protect 123
  w4.Protect DrawingObjects:=False, Contents:=True, Scenarios:=False
  w5.Protect DrawingObjects:=True, Contents:=False, Scenarios:=True
  t = "w1" & St(w1) & " w2" & St(w2) & " w3" & St(w3) & " w4" & St(w4) & " w5" & St(w5)
  ActiveWorkbook.SaveAs Environ("TEMP") & "\ww24g10.xlsx", 51: Note "save"
 End Select
 Probe = t
End Function
