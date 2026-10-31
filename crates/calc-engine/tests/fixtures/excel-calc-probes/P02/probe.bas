Private Function CaseIds() As String
    Dim mode As Variant, op As Variant, t As String
    For Each mode In Array("manual", "auto", "except")
        For Each op In Array("c1", "range", "sequential", "union-cb", "union-bc", "sheet", "app", "full", "rebuild", "rowmajor")
            If Len(t) > 0 Then t = t & "|"
            t = t & CStr(mode) & "-" & CStr(op)
        Next
    Next
    CaseIds = t & "|cross-sheet-c1|cross-sheet-sheet|cross-sheet-app|cross-book-c1|cross-book-sheet|cross-book-app|reverse-range|reverse-rowmajor|disabled-sheet|disabled-full|forcefull-c1|forcefull-app|group-sheet"
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim ws As Worksheet, other As Worksheet, provider As Workbook, b As Range, c As Range, p As String, op As String
    Set ws = observedSheet
    p = Split(ident, "/")(0)
    op = Mid$(p, InStrRev(p, "-") + 1)
    Stage "setup-entry"
    ws.Range("A1").Value2 = 1
    ws.Range("B1").Formula2 = "=A1*2"
    ws.Range("C1").Formula2 = "=B1+1"
    ws.Range("B3").Formula2 = "=PlainProbe(32,A1)"
    ws.Range("C3").Formula2 = "=PlainProbe(33,B3)"
    ws.Range("K1").Formula2 = "=PlainProbe(34,1)"
    ws.Range("X1").Value2 = 0
    ws.Range("Y1").Formula2 = "=X1+1"
    Set b = ws.Range("B1")
    Set c = ws.Range("C1")
    If Left$(p, 11) = "cross-sheet" Then
        Set other = ThisWorkbook.Worksheets.Add(After:=ws)
        other.Name = "Producer"
        other.Range("A1").Value2 = 1
        other.Range("B1").Formula2 = "=A1*2"
        ws.Range("B1").ClearContents
        c.Formula2 = "=Producer!B1+1"
        Set b = other.Range("B1")
    ElseIf Left$(p, 10) = "cross-book" Then
        Set provider = Application.Workbooks.Add(xlWBATWorksheet)
        Set other = provider.Worksheets(1)
        other.Range("A1").Value2 = 1
        other.Range("B1").Formula2 = "=A1*2"
        provider.SaveAs Replace(output, ".xlsm", "-provider.xlsx"), xlOpenXMLWorkbook
        ws.Range("B1").ClearContents
        c.Formula2 = "='[" & provider.Name & "]" & other.Name & "'!B1+1"
        Set b = other.Range("B1")
        ThisWorkbook.Activate
    ElseIf Left$(p, 7) = "reverse" Then
        ws.Range("A1:C1").ClearContents
        ws.Range("C1").Value2 = 1
        ws.Range("B1").Formula2 = "=C1*2"
        ws.Range("A1").Formula2 = "=B1+1"
    End If
    Stage "seed"
    Application.CalculateFullRebuild
    Application.Calculate
    Snap ws, "A1:C1,B3:C3,K1,X1:Y1"
    CountSnapshot
    If Left$(p, 7) <> "reverse" Then
        If c.Value2 <> 3 Or b.Value2 <> 2 Then Err.Raise 5, , "Invalid chain seed"
    End If
    If Left$(p, 4) = "auto" Then
        Stage "mode-switch-auto"
        Application.Calculation = xlCalculationAutomatic
    ElseIf Left$(p, 6) = "except" Then
        Stage "mode-switch-except"
        Application.Calculation = xlCalculationSemiautomatic
    End If
    If Left$(p, 8) = "disabled" Then ws.EnableCalculation = False
    If Left$(p, 9) = "forcefull" Then ThisWorkbook.ForceFullCalculation = True
    If p = "group-sheet" Then
        Set other = ThisWorkbook.Worksheets.Add(After:=ws)
        other.Name = "Grouped"
        other.Range("A1").Formula2 = "=PlainProbe(35,S!A1)"
        Application.Calculate
        ThisWorkbook.Worksheets(Array("S", "Grouped")).Select
    End If
    Stage "dirty-input"
    If Not other Is Nothing And Left$(p, 5) = "cross" Then
        other.Range("A1").Value2 = 5
    ElseIf Left$(p, 7) = "reverse" Then
        ws.Range("C1").Value2 = 5
    Else
        ws.Range("A1").Value2 = 5
    End If
    ws.Range("X1").Value2 = 1
    Snap ws, "A1:C1,B3:C3,K1,Y1"
    CountSnapshot
    Stage "tested-command"
    If Left$(p, 7) = "reverse" Then
        If op = "range" Then ws.Range("A1:B1").Calculate Else ws.Range("A1:B1").CalculateRowMajorOrder
    ElseIf InStr(p, "union-cb") > 0 Then
        Application.Union(c, b, ws.Range("C3"), ws.Range("B3")).Calculate
    ElseIf InStr(p, "union-bc") > 0 Then
        Application.Union(b, c, ws.Range("B3"), ws.Range("C3")).Calculate
    Else
        Select Case op
        Case "c1": c.Calculate: ws.Range("C3").Calculate
        Case "range": ws.Range("B1:C1,B3:C3").Calculate
        Case "sequential": b.Calculate: ws.Range("B3").Calculate: c.Calculate: ws.Range("C3").Calculate
        Case "sheet": ws.Calculate
        Case "app": Application.Calculate
        Case "full": Application.CalculateFull
        Case "rebuild": Application.CalculateFullRebuild
        Case "rowmajor": ws.Range("B1:C1,B3:C3").CalculateRowMajorOrder
        End Select
    End If
    Stage "after-command"
    Snap ws, "A1:C1,B3:C3,K1,Y1"
    If Left$(p, 5) = "cross" Then Snap other, "A1:B1"
    CountSnapshot
    Stage "followup-f9"
    Application.Calculate
    Snap ws, "A1:C1,B3:C3,K1,Y1"
    CountSnapshot
    Stage "followup-full"
    Application.CalculateFull
    Snap ws, "A1:C1,B3:C3,K1,Y1"
    CountSnapshot
    If Not provider Is Nothing Then provider.Close False
    Application.Calculation = xlCalculationManual
End Sub
