Private Declare PtrSafe Function DumpNative Lib "__NATIVE__" (ByVal path As LongPtr) As Long
Private Declare PtrSafe Function ResetNative Lib "__NATIVE__" () As Long
Private Declare PtrSafe Function MarkNative Lib "__NATIVE__" (ByVal stageNumber As Long) As Long
Private Declare PtrSafe Function ObserveNative Lib "__NATIVE__" (ByVal observedType As Long, ByVal observedValue As Double, ByVal site As Long) As Long
Private Function CaseIds() As String
    Dim entry As Variant, chain As Variant, mtc As Variant, t As String
    For Each entry In Array("producer", "observer")
        For Each chain In Array("cold", "warm", "rebuild")
            For Each mtc In Array("on", "off")
                If Len(t) > 0 Then t = t & "|"
                t = t & CStr(entry) & "-" & CStr(chain) & "-" & CStr(mtc)
            Next
        Next
    Next
    CaseIds = t & "|producer-cold-other-active|observer-cold-other-active|producer-cold-scroll|observer-cold-scroll|producer-cold-lower-chain|observer-cold-lower-chain|producer-cold-one-thread|observer-cold-one-thread"
End Function
Public Function PeekSeveral() As Variant
    Dim a As Variant, b As Variant, c As Variant, n As Long
    a = observedSheet.Range("B1").Value2
    b = observedSheet.Range("D1").Value2
    c = observedSheet.Range("H1").Value2
    n = ObserveNative(VarType(a), CDbl(a), 1)
    n = ObserveNative(VarType(b), CDbl(b), 2)
    n = ObserveNative(VarType(c), CDbl(c), 3)
    Note "entry", "PeekSeveral;B1=" & Typed(a) & ";D1=" & Typed(b) & ";H1=" & Typed(c)
    PeekSeveral = CStr(a) & ":" & CStr(b) & ":" & CStr(c)
    Note "exit", "PeekSeveral"
End Function
Private Sub EnterObservers(ws As Worksheet)
    ws.Range("C1").Formula2 = "=PeekB1()"
    ws.Range("F1").Formula2 = "=PeekSeveral()"
End Sub
Private Sub EnterProducers(ws As Worksheet, ByVal firstRow As Long)
    ws.Cells(firstRow, 11).Value2 = 1
    ws.Range(ws.Cells(firstRow + 1, 11), ws.Cells(firstRow + 999, 11)).FormulaR1C1 = "=R[-1]C+1"
    ws.Cells(firstRow, 12).Value2 = 100
    ws.Range(ws.Cells(firstRow + 1, 12), ws.Cells(firstRow + 999, 12)).FormulaR1C1 = "=R[-1]C+1"
    ws.Range("B1").Formula2 = "=K" & CStr(firstRow + 999) & "+1"
    ws.Range("D1").Formula2 = "=L" & CStr(firstRow + 999) & "+1"
    ws.Range("H1").Formula2 = "=XPure(K" & CStr(firstRow + 999) & "+1)"
End Sub
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim ws As Worksheet, other As Worksheet, p As String, firstRow As Long, n As Long, path As String, oldThreadMode As Long
    Set ws = observedSheet
    p = Split(ident, "/")(0)
    firstRow = 1
    If InStr(p, "lower-chain") > 0 Then firstRow = 2000
    Stage "setup-entry-history"
    Note "register-xll", CStr(Application.RegisterXLL("__NATIVE__"))
    n = ResetNative()
    n = MarkNative(1)
    If Left$(p, 8) = "observer" Then EnterObservers ws
    EnterProducers ws, firstRow
    If Left$(p, 8) <> "observer" Then EnterObservers ws
    Application.CalculateFull
    ws.Range("B1,D1,H1").Calculate
    ws.Range("C1,F1").Calculate
    Stage "accepted-seed"
    Snap ws, "B1:D1,F1,H1"
    If ws.Range("B1").Value2 <> 1001 Or ws.Range("C1").Value2 <> 1001 Then Err.Raise 5, , "Invalid observer seed"
    If InStr(p, "warm") > 0 Then Application.Calculate: Application.Calculate
    If Right$(p, 3) = "off" Then Application.MultiThreadedCalculation.Enabled = False Else Application.MultiThreadedCalculation.Enabled = True
    If InStr(p, "one-thread") > 0 Then
        oldThreadMode = Application.MultiThreadedCalculation.ThreadMode
        Application.MultiThreadedCalculation.ThreadMode = xlThreadModeManual
        On Error Resume Next
        CallByName Application.MultiThreadedCalculation, "ThreadCount", VbLet, 1
        Note "thread-count-set", "error=" & CStr(Err.Number) & ";" & Err.Description & ";actual=" & CStr(Application.MultiThreadedCalculation.ThreadCount)
        Err.Clear
        On Error GoTo 0
        If Application.MultiThreadedCalculation.ThreadCount <> 1 Then
            Application.MultiThreadedCalculation.ThreadMode = oldThreadMode
            Note "unrunnable", "One-thread MTC count cannot be set through this COM property; no UI/registry setting changed"
            Exit Sub
        End If
    End If
    If InStr(p, "other-active") > 0 Then
        Set other = ThisWorkbook.Worksheets.Add(After:=ws)
        other.Name = "ActiveOther"
        other.Activate
    Else
        ws.Activate
    End If
    If InStr(p, "scroll") > 0 Then
        ws.Range("Z3000").Select
        ActiveWindow.ScrollRow = 2990
    Else
        ActiveSheet.Range("A1").Select
        ActiveWindow.ScrollRow = 1
    End If
    Stage "dirty"
    ws.Cells(firstRow, 11).Value2 = 10
    ws.Cells(firstRow, 12).Value2 = 200
    Note "layout", "activeSheet=" & ActiveSheet.Name & ";selection=" & Selection.Address & ";scrollRow=" & CStr(ActiveWindow.ScrollRow) & ";firstChainRow=" & CStr(firstRow) & ";mtc=" & CStr(Application.MultiThreadedCalculation.Enabled)
    Stage "tested-command"
    n = MarkNative(2)
    If InStr(p, "rebuild") > 0 Then Application.CalculateFullRebuild Else Application.CalculateFull
    Stage "after"
    Snap ws, "B1:D1,F1,H1"
    CountSnapshot
    path = Replace(output, ".xlsm", "-native.tsv")
    n = DumpNative(StrPtr(path))
    Note "native-records", CStr(n)
    Application.MultiThreadedCalculation.Enabled = True
    If InStr(p, "one-thread") > 0 Then Application.MultiThreadedCalculation.ThreadMode = oldThreadMode
End Sub
