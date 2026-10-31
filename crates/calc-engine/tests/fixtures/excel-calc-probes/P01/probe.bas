Private Function CaseIds() As String
    CaseIds = "cold-c1|cold-b1-c1|cold-contiguous|cold-reverse|cold-sheet|cold-app|cold-full|cold-rebuild|warm-c1|warm-b1-c1|warm-contiguous|warm-reverse|warm-sheet|warm-app|warm-full|warm-rebuild|counter-c1|counter-reverse|counter-sheet|counter-app|counter-full|counter-rebuild|self-fresh|self-held|twice"
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim ws As Worksheet, op As String, v As Variant
    Set ws = observedSheet
    op = Split(Split(ident, "/")(0), "-")(UBound(Split(Split(ident, "/")(0), "-")))
    Stage "setup-entry"
    ws.Range("A1").Value2 = 1
    If Left$(ident, 7) = "counter" Then
        ws.Range("B1").Formula2 = "=PlainProbe(31,A1)+1"
    Else
        ws.Range("B1").Formula2 = "=A1+1"
    End If
    ws.Range("C1").Formula2 = "=PeekB1()"
    Stage "seed"
    ws.Range("B1").Calculate
    ws.Range("C1").Calculate
    Snap ws, "A1,B1,C1"
    If ws.Range("B1").Value2 <> 2 Or ws.Range("C1").Value2 <> 2 Then Err.Raise 5, , "Invalid seed"
    If Left$(ident, 4) = "warm" Then
        Stage "warm-chain"
        Application.Calculate
        Application.Calculate
    End If
    Stage "dirty"
    ws.Range("A1").Value2 = 10
    Snap ws, "A1,B1,C1"
    v = Application.ExecuteExcel4Macro("S!R1C2")
    Note "xlf-read", Typed(v)
    Stage "tested-command"
    If InStr(ident, "b1-c1") > 0 Then
        ws.Range("B1").Calculate
        ws.Range("C1").Calculate
    ElseIf Left$(ident, 4) = "self" Then
        If InStr(ident, "held") > 0 Then
            ws.Range("C2").Value2 = 17
        End If
        ws.Range("C2").Formula2 = "=PeekSelf()"
        ws.Range("C2").Calculate
        ws.Range("C2").Calculate
        Snap ws, "C2"
    Else
        Select Case op
        Case "c1": ws.Range("C1").Calculate
        Case "contiguous": ws.Range("B1:C1").Calculate
        Case "reverse": ws.Range("C1,B1").Calculate
        Case "sheet": ws.Calculate
        Case "app": Application.Calculate
        Case "full": Application.CalculateFull
        Case "rebuild": Application.CalculateFullRebuild
        Case "twice"
            ws.Range("C3").Formula2 = "=PeekTwice()"
            ws.Range("C3").Calculate
            Snap ws, "C3"
        End Select
    End If
    Stage "after"
    Snap ws, "A1,B1,C1"
    CountSnapshot
End Sub
