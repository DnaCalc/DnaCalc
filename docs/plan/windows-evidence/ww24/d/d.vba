Public Lg As String
Public Sub L(x As String)
 Lg = Lg & x & ">"
End Sub
Private Function Er() As String
 Er = "ERR " & Err.Number & "|" & Err.Description
End Function
Public Sub Inject(wb As Workbook)
 Dim p As Object, h As String, s As String
 h = "Private Sub Workbook_SheetActivate(ByVal Sh As Object)" & vbCrLf & "On Error Resume Next" & vbCrLf & "Application.Run ""'" & ThisWorkbook.Name & "'!LaneProbe.L"", ""WB.SheetActivate:"" & Sh.Name" & vbCrLf & "End Sub" & vbCrLf
 h = h & "Private Sub Workbook_SheetDeactivate(ByVal Sh As Object)" & vbCrLf & "On Error Resume Next" & vbCrLf & "Application.Run ""'" & ThisWorkbook.Name & "'!LaneProbe.L"", ""WB.SheetDeactivate:"" & Sh.Name" & vbCrLf & "End Sub" & vbCrLf
 h = h & "Private Sub Workbook_NewSheet(ByVal Sh As Object)" & vbCrLf & "On Error Resume Next" & vbCrLf & "Application.Run ""'" & ThisWorkbook.Name & "'!LaneProbe.L"", ""WB.NewSheet:"" & Sh.Name" & vbCrLf & "End Sub" & vbCrLf
 h = h & "Private Sub Workbook_SheetCalculate(ByVal Sh As Object)" & vbCrLf & "On Error Resume Next" & vbCrLf & "Application.Run ""'" & ThisWorkbook.Name & "'!LaneProbe.L"", ""WB.SheetCalculate:"" & Sh.Name" & vbCrLf & "End Sub" & vbCrLf
 h = h & "Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)" & vbCrLf & "On Error Resume Next" & vbCrLf & "Application.Run ""'" & ThisWorkbook.Name & "'!LaneProbe.L"", ""WB.BeforeSave""" & vbCrLf & "Cancel = True" & vbCrLf & "End Sub" & vbCrLf
 h = h & "Private Sub Workbook_BeforeClose(Cancel As Boolean)" & vbCrLf & "On Error Resume Next" & vbCrLf & "Application.Run ""'" & ThisWorkbook.Name & "'!LaneProbe.L"", ""WB.BeforeClose""" & vbCrLf & "End Sub" & vbCrLf
 wb.VBProject.VBComponents("ThisWorkbook").CodeModule.AddFromString h
 s = "Private Sub Worksheet_Activate()" & vbCrLf & "On Error Resume Next" & vbCrLf & "Application.Run ""'" & ThisWorkbook.Name & "'!LaneProbe.L"", ""WS.Activate:"" & Me.Name" & vbCrLf & "End Sub" & vbCrLf
 s = s & "Private Sub Worksheet_Deactivate()" & vbCrLf & "On Error Resume Next" & vbCrLf & "Application.Run ""'" & ThisWorkbook.Name & "'!LaneProbe.L"", ""WS.Deactivate:"" & Me.Name" & vbCrLf & "End Sub" & vbCrLf
 s = s & "Private Sub Worksheet_Calculate()" & vbCrLf & "On Error Resume Next" & vbCrLf & "Application.Run ""'" & ThisWorkbook.Name & "'!LaneProbe.L"", ""WS.Calculate:"" & Me.Name" & vbCrLf & "End Sub" & vbCrLf
 For Each p In wb.Worksheets
  wb.VBProject.VBComponents(p.CodeName).CodeModule.AddFromString s
 Next
End Sub
Public Function Probe(k As Long) As String
 Dim wb As Workbook, b2 As Workbook, t As String, n As Long
 Application.EnableEvents = True
 Set wb = Workbooks.Add
 On Error GoTo Failed
 Select Case k
 Case 1
  wb.Worksheets.Add After:=wb.Worksheets(1)
  Inject wb
  Lg = ""
  wb.Worksheets(2).Activate: t = "to2: " & Lg: Lg = ""
  wb.Worksheets(1).Activate: t = t & " | to1: " & Lg: Lg = ""
  wb.Worksheets.Add: t = t & " | add: " & Lg: Lg = ""
  wb.Worksheets.Add After:=wb.Worksheets(wb.Worksheets.Count): t = t & " | addAfter: " & Lg
  Lg = "": wb.Worksheets(1).Select: t = t & " | select1: " & Lg
  Probe = t
 Case 2
  wb.SaveAs Environ("TEMP") & "\ww24d2.xlsm", 52
  Inject wb
  Lg = ""
  wb.Worksheets(1).Range("A1").Value = 5
  On Error Resume Next
  wb.Close True
  t = "close err=" & Err.Number & "|" & Err.Description & " log=" & Lg & " open=" & Workbooks.Count
  Err.Clear
  On Error GoTo Failed
  Probe = t & " wbStillOpen=" & wb.Name & " saved=" & wb.Saved
  wb.Saved = True
  Lg = ""
  wb.Close False
  Probe = Probe & " | closeFalse log=" & Lg
 Case 3, 4, 5
  Inject wb
  wb.Worksheets(1).Range("A1").Value = 1: wb.Worksheets(1).Range("A2").Formula = "=A1+1": wb.Worksheets(1).Range("A3").Formula = "=RAND()"
  Set b2 = Workbooks.Add
  Inject b2
  b2.Worksheets(1).Range("A1").Value = 1: b2.Worksheets(1).Range("A2").Formula = "=A1+1": b2.Worksheets(1).Range("A3").Formula = "=RAND()"
  If k >= 4 Then Application.Calculation = xlCalculationManual
  If k = 5 Then Application.CalculateBeforeSave = True
  b2.Worksheets(1).Range("A1").Value = 10
  Dim r1 As Double, r2 As Double
  r1 = b2.Worksheets(1).Range("A3").Value: r2 = wb.Worksheets(1).Range("A3").Value
  wb.Activate
  Lg = ""
  Application.Calculate
  t = "Calculate: log=" & Lg & " b2.A2=" & b2.Worksheets(1).Range("A2").Value & " b2.rand changed=" & (b2.Worksheets(1).Range("A3").Value <> r1) & " wb.rand changed=" & (wb.Worksheets(1).Range("A3").Value <> r2) & " | "
  b2.Worksheets(1).Range("A1").Value = 20: r1 = b2.Worksheets(1).Range("A3").Value: r2 = wb.Worksheets(1).Range("A3").Value
  Lg = ""
  Application.CalculateFull
  t = t & "CalculateFull: log=" & Lg & " b2.A2=" & b2.Worksheets(1).Range("A2").Value & " b2.rand changed=" & (b2.Worksheets(1).Range("A3").Value <> r1) & " | "
  b2.Worksheets(1).Range("A1").Value = 30: r1 = b2.Worksheets(1).Range("A3").Value
  Lg = ""
  Application.CalculateFullRebuild
  t = t & "CalculateFullRebuild: log=" & Lg & " b2.A2=" & b2.Worksheets(1).Range("A2").Value & " b2.rand changed=" & (b2.Worksheets(1).Range("A3").Value <> r1) & " | "
  If k = 5 Then
   b2.SaveAs Environ("TEMP") & "\ww24d5.xlsm", 52
   t = t & "afterSaveAttempt log=" & Lg
  End If
  Application.Calculation = xlCalculationAutomatic
  Probe = t
  b2.Saved = True: b2.Close False
 End Select
 Exit Function
Failed:
 Probe = t & " " & Er()
End Function



