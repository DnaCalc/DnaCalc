Private armed As Boolean
Private sizes As String
Private sizeCalls As Long
Private makeVolatile As Boolean
Private alternateSizes As Boolean
Private errorAt As Long
Private seedSize As Long
Private Function CaseIds() As String
    Dim t As String, mode As Variant, script As Variant, op As Variant
    t = "draws-entry|draws-f9|draws-range|draws-full|draws-rebuild"
    For Each mode In Array("plain", "volatile")
        For Each script In Array("stable", "settles", "alternates", "late-change", "error")
            For Each op In Array("f9", "range", "full", "rebuild")
                t = t & "|" & CStr(mode) & "-" & CStr(script) & "-" & CStr(op)
            Next
        Next
    Next
    CaseIds = t & "|volatile-shifted-full|volatile-shifted-rebuild|volatile-alternates-entry|plain-alternates-entry"
End Function
Private Sub FullSpill(ws As Worksheet, ByVal atCell As String)
    SpillInfo ws, atCell
    On Error GoTo NoArray
    Dim r As Range
    Set r = ws.Range(atCell).SpillingToRange
    Snap ws, r.Address
    Exit Sub
NoArray:
    Err.Clear
End Sub
Public Function SizeUdf() As Variant
    Dim sequence As Variant, at As Long, v As Long
    If makeVolatile Then Application.Volatile
    calls(51) = calls(51) + 1
    If Not armed Then
        v = seedSize
        Note "size", "unarmed;n=" & CStr(calls(51)) & ";returned=" & CStr(v)
    Else
        sizeCalls = sizeCalls + 1
        If errorAt > 0 And sizeCalls = errorAt Then
            Note "size", "armed;n=" & CStr(sizeCalls) & ";returned=Error2015"
            SizeUdf = CVErr(xlErrValue)
            Exit Function
        End If
        If alternateSizes Then
            v = 2 + ((sizeCalls - 1) Mod 2)
        Else
            sequence = Split(sizes, ",")
            at = sizeCalls - 1
            If at > UBound(sequence) Then at = UBound(sequence)
            v = CLng(sequence(at))
        End If
        Note "size", "armed;n=" & CStr(sizeCalls) & ";returned=" & CStr(v)
    End If
    SizeUdf = v
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim ws As Worksheet, p As String, op As String
    Set ws = observedSheet
    p = Split(ident, "/")(0)
    op = Mid$(p, InStrRev(p, "-") + 1)
    Stage "setup-entry"
    If Left$(p, 5) = "draws" Then
        ThisWorkbook.Names.Add Name:="DrawName", RefersTo:="=RAND()"
        ws.Range("A1").Formula2 = "=RAND()-RAND()"
        ws.Range("A3").Formula2 = "=LET(draw,RAND(),draw-draw)"
        ws.Range("A5").Formula2 = "=LAMBDA(x,x-x)(RAND())"
        ws.Range("A7:A8").Formula2 = "=DrawName-DrawName"
        ws.Range("A9").Formula2 = "=LET(fn,LAMBDA(RAND()),fn()-fn())"
        ws.Range("D1").Formula2 = "=MAP(SEQUENCE(4),LAMBDA(x,RAND()))"
        ws.Range("E1").Formula2 = "=LET(draw,RAND(),MAP(SEQUENCE(4),LAMBDA(x,draw)))"
        ws.Range("G1").Formula2 = "=RANDARRAY(2,3)"
        ws.Range("N1").Formula2 = "=SEQUENCE(RANDBETWEEN(1,1000))"
        Stage "after-entry"
        Snap ws, "A1,A3,A5,A7:A9,D1:E4,G1:I2,N1"
    Else
        makeVolatile = Left$(p, 8) = "volatile"
        seedSize = 2
        If InStr(p, "changed-seed") > 0 Then seedSize = 1
        sizes = "2"
        If InStr(p, "settles") > 0 Then sizes = "2,3,3,3"
        If InStr(p, "late-change") > 0 Then sizes = "2,2,3"
        If InStr(p, "alternates") > 0 Then alternateSizes = True
        If InStr(p, "shifted") > 0 Then sizes = "3,2,3"
        If InStr(p, "error") > 0 Then errorAt = 3: alternateSizes = True
        ws.Range("A1").Formula2 = "=SEQUENCE(SizeUdf())"
        ws.Range("D1").Formula2 = "=PlainProbe(52,SUM(A1#))"
        ws.Range("F1").Formula2 = "=SEQUENCE(2)"
        Application.CalculateFull
        Stage "accepted-seed"
        Snap ws, "A1:A4,D1,F1:F4"
        SpillInfo ws, "A1"
        armed = True
        sizeCalls = 0
    End If
    Stage "tested-command"
    Select Case op
    Case "f9": Application.Calculate
    Case "range": ws.UsedRange.Calculate
    Case "full": Application.CalculateFull
    Case "rebuild": Application.CalculateFullRebuild
    Case "entry": ws.Range("A1").ClearContents: ws.Range("A1").Formula2 = "=SEQUENCE(SizeUdf())"
    End Select
    Stage "after"
    If Left$(p, 5) = "draws" Then
        Snap ws, "A1,A3,A5,A7:A9,D1:E4,G1:I2,N1"
        FullSpill ws, "D1"
        FullSpill ws, "E1"
        FullSpill ws, "G1"
        FullSpill ws, "N1"
    Else
        Snap ws, "A1:A12,D1,F1:F4"
        SpillInfo ws, "A1"
        Note "size-total", CStr(sizeCalls)
        CountSnapshot
    End If
End Sub
