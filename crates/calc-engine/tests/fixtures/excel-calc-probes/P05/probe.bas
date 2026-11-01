Private Function CaseIds() As String
    CaseIds = "growth-reader|growth-anchor|growth-both|growth-sheet|growth-full|growth-child|conflict-ab-cold|conflict-ba-cold|conflict-ab-warm|conflict-ba-warm|conflict-ab-rebuild|conflict-ba-rebuild|conflict-ab-single-a|conflict-ab-single-b|conflict-ab-mtc-off|conflict-ba-mtc-off|conflict-ab-dirty-reverse|literal|empty-formula|formatted-blank|table|grid-edge|existing-child-growth|existing-child-shrink|growth-reader-first"
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim ws As Worksheet, p As String, anchor As Range, lo As ListObject, op As String
    Set ws = observedSheet
    p = Split(ident, "/")(0)
    Stage "setup"
    If Left$(p, 14) = "existing-child" Then
        ws.Range("Z1").Value2 = 10
        ws.Range("A1").Formula2 = "=SEQUENCE(Z1)"
        ws.Range("C10").Formula2 = "=A10+1"
        Application.CalculateFull
        Stage "accepted-seed"
        Snap ws, "A1:A14,C10"
        SpillInfo ws, "A1"
        If ws.Range("A10").Value2 <> 10 Then Err.Raise 5, , "Invalid child seed"
        Stage "dirty"
        If p = "existing-child-growth" Then ws.Range("Z1").Value2 = 12 Else ws.Range("Z1").Value2 = 1
        Stage "tested-existing-child-only"
        ws.Range("A10").Calculate
        Stage "after-child-only"
        Snap ws, "A1:A14,C10"
        SpillInfo ws, "A1"
        Stage "followup-f9"
        Application.Calculate
        Snap ws, "A1:A14,C10"
        SpillInfo ws, "A1"
    ElseIf Left$(p, 6) = "growth" Then
        ws.Range("Z1").Value2 = 1
        ws.Range("A1").Formula2 = "=SEQUENCE(Z1)"
        ws.Range("C10").Formula2 = "=A10+1"
        ws.Range("E10").Formula2 = "=PlainProbe(60,A10+1)"
        Application.CalculateFull
        Application.Calculate
        Stage "accepted-seed"
        Snap ws, "A1:A12,C10,E10"
        SpillInfo ws, "A1"
        Stage "dirty-growth"
        ws.Range("Z1").Value2 = 10
        Stage "tested-command"
        op = Mid$(p, InStrRev(p, "-") + 1)
        Select Case op
        Case "reader": ws.Range("C10,E10").Calculate
        Case "anchor": ws.Range("A1").Calculate
        Case "both": ws.Range("A1,C10,E10").Calculate
        Case "sheet": ws.Calculate
        Case "full": Application.CalculateFull
        Case "child": ws.Range("A10").Calculate
        Case "first": ws.Range("C10,E10,A1").Calculate
        End Select
        Stage "after-growth"
        Snap ws, "A1:A12,C10,E10"
        SpillInfo ws, "A1"
        CountSnapshot
        Stage "fill-before-shrink"
        Application.CalculateFull
        Snap ws, "A1:A12,C10,E10"
        Stage "shrink-anchor-reader"
        ws.Range("Z1").Value2 = 1
        ws.Range("A1,C10,E10").Calculate
        Snap ws, "A1:A12,C10,E10"
        SpillInfo ws, "A1"
        Stage "shrink-f9"
        Application.Calculate
        Snap ws, "A1:A12,C10,E10"
    ElseIf Left$(p, 8) = "conflict" Then
        ws.Range("Z1").Value2 = 0
        If InStr(p, "-ba-") > 0 Then
            ws.Range("B1").Formula2 = "=IF(Z1,SEQUENCE(4,3),1)"
            ws.Range("A3").Formula2 = "=IF(Z1,SEQUENCE(4,3),1)"
        Else
            ws.Range("A3").Formula2 = "=IF(Z1,SEQUENCE(4,3),1)"
            ws.Range("B1").Formula2 = "=IF(Z1,SEQUENCE(4,3),1)"
        End If
        ws.Range("F1").Formula2 = "=SUM(B3:C4)"
        Application.CalculateFull
        If InStr(p, "warm") > 0 Then Application.Calculate: Application.Calculate
        If InStr(p, "mtc-off") > 0 Then Application.MultiThreadedCalculation.Enabled = False
        Stage "accepted-seed"
        Snap ws, "A3,B1,F1"
        Stage "dirty"
        ws.Range("Z1").Value2 = 1
        If InStr(p, "dirty-reverse") > 0 Then ws.Range("B1").Dirty: ws.Range("A3").Dirty
        Stage "tested-command"
        If InStr(p, "single-a") > 0 Then
            ws.Range("A3").Calculate
        ElseIf InStr(p, "single-b") > 0 Then
            ws.Range("B1").Calculate
        ElseIf InStr(p, "rebuild") > 0 Then
            Application.CalculateFullRebuild
        Else
            Application.Calculate
        End If
        Stage "after-conflict"
        Snap ws, "A1:D6,F1"
        SpillInfo ws, "A3"
        SpillInfo ws, "B1"
        Stage "followup-full"
        Application.CalculateFull
        Snap ws, "A3,B1,F1"
        SpillInfo ws, "A3"
        SpillInfo ws, "B1"
        Application.MultiThreadedCalculation.Enabled = True
    Else
        Set anchor = ws.Range("A1")
        Select Case p
        Case "literal": ws.Range("A2").Value2 = 9
        Case "empty-formula": ws.Range("A2").Formula2 = "="""""
        Case "formatted-blank": ws.Range("A2").NumberFormat = "0.00": ws.Range("A2").Interior.Color = vbYellow
        Case "table"
            ws.Range("A1").Value2 = "Header"
            Set lo = ws.ListObjects.Add(xlSrcRange, ws.Range("A1:B4"), , xlYes)
            Set anchor = ws.Range("A2")
        Case "grid-edge": Set anchor = ws.Range("A1048576")
        End Select
        Stage "entry-and-full"
        anchor.Formula2 = "=SEQUENCE(3)"
        Application.CalculateFull
        Snap ws, anchor.Address & ",A1:B5"
        SpillInfo ws, anchor.Address
    End If
End Sub
