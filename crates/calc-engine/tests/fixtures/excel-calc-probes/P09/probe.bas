Private Function CaseIds() As String
    CaseIds = "formula|formula2|explicit-at|cse|paste-formula|paste-formula2|fill-formula|fill-formula2|r1c1|r1c1-formula2"
End Function
Private Sub FormulaInfo(ws As Worksheet, ByVal atCell As String)
    Dim r As Range
    Set r = ws.Range(atCell)
    Note "formula-form", atCell & ";Formula=" & CStr(r.Formula) & ";Formula2=" & CStr(r.Formula2) & ";R1C1=" & CStr(r.FormulaR1C1) & ";Formula2R1C1=" & CStr(r.Formula2R1C1)
    Note "array-flags", atCell & ";hasArray=" & Typed(r.HasArray) & ";hasSpill=" & Typed(r.HasSpill) & ";savedAsArray=" & Typed(r.SavedAsArray)
    SpillInfo ws, atCell
End Sub
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim ws As Worksheet, p As String, r As Long
    Set ws = observedSheet
    p = Split(ident, "/")(0)
    Stage "setup"
    ws.Range("A1").Value2 = 1
    ws.Range("A2").Value2 = 4
    ws.Range("A3").Value2 = 9
    ws.Range("A4").Value2 = 16
    ws.Range("C1:C4").Value2 = ws.Range("A1:A4").Value2
    Stage "tested-ingress"
    On Error GoTo IngressError
    Select Case p
    Case "formula": ws.Range("B2").Formula = "=SQRT(A1:A4)"
    Case "formula2": ws.Range("B2").Formula2 = "=SQRT(A1:A4)"
    Case "explicit-at": ws.Range("B2").Formula2 = "=SQRT(@A1:A4)"
    Case "cse": ws.Range("B2:B5").FormulaArray = "=SQRT(A1:A4)"
    Case "paste-formula"
        ws.Range("D2").Formula = "=SQRT(C1:C4)"
        ws.Range("D2").Copy Destination:=ws.Range("B2")
    Case "paste-formula2"
        ws.Range("D2").Formula2 = "=SQRT(C1:C4)"
        ws.Range("D2").Copy Destination:=ws.Range("B2")
    Case "fill-formula"
        ws.Range("B2").Formula = "=SQRT(A1:A4)"
        ws.Range("B2:B5").FillDown
    Case "fill-formula2"
        ws.Range("B2").Formula2 = "=SQRT(A1:A4)"
        ws.Range("B2:B5").FillDown
    Case "r1c1": ws.Range("B2").FormulaR1C1 = "=SQRT(R[-1]C[-1]:R[2]C[-1])"
    Case "r1c1-formula2": ws.Range("B2").Formula2R1C1 = "=SQRT(R[-1]C[-1]:R[2]C[-1])"
    End Select
    GoTo AfterIngress
IngressError:
    Note "ingress-error", CStr(Err.Number) & ";" & Err.Description
    Err.Clear
    Resume AfterIngress
AfterIngress:
    On Error GoTo 0
    Application.CutCopyMode = False
    Stage "after-ingress"
    For r = 2 To 5
        FormulaInfo ws, "B" & CStr(r)
    Next
    Note "array-flags-mixed", "B2:B5;savedAsArray=" & Typed(ws.Range("B2:B5").SavedAsArray)
    Stage "calculate-full"
    Application.CalculateFull
    Snap ws, "A1:D6"
    For r = 2 To 5
        FormulaInfo ws, "B" & CStr(r)
    Next
End Sub
