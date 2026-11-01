Option Explicit
Private held As Range
Private logLines(0 To 4095) As String
Private used As Long
Private holdCalls As Long
Private readCalls As Long
Private fCalls As Long
Private lastStatic As Long
Private Sub Record(ByVal detail As String)
    If used <= UBound(logLines) Then logLines(used) = detail: used = used + 1
End Sub
Public Function Hold(r As Range) As Variant
    holdCalls = holdCalls + 1
    Set held = r
    Record "Hold;n=" & CStr(holdCalls) & ";address=" & held.Address(External:=True) & ";areas=" & CStr(held.Areas.Count)
    Hold = r.Value2
End Function
Public Function ReadHeld() As Variant
    On Error GoTo Failed
    Dim v As Variant
    readCalls = readCalls + 1
    If held Is Nothing Then
        Record "ReadHeld;n=" & CStr(readCalls) & ";held=Nothing"
        ReadHeld = CVErr(xlErrRef)
        Exit Function
    End If
    v = held.Value2
    Record "ReadHeld;n=" & CStr(readCalls) & ";parent=" & held.Parent.Parent.Name & ";address=" & held.Address(External:=True) & ";areas=" & CStr(held.Areas.Count) & ";type=" & TypeName(held) & ";value-vt=" & CStr(VarType(v)) & ";value=" & CStr(v)
    ReadHeld = v
    Exit Function
Failed:
    Record "ReadHeld;n=" & CStr(readCalls) & ";error=" & CStr(Err.Number) & ";" & Err.Description
    ReadHeld = CVErr(xlErrRef)
End Function
Public Function Stateful() As Long
    Static n As Long
    n = n + 1
    lastStatic = n
    Record "Stateful;n=" & CStr(n)
    Stateful = n
End Function
Public Function F(ByVal x As Double) As Variant
    fCalls = fCalls + 1
    Record "F;n=" & CStr(fCalls) & ";arg-type=" & TypeName(x) & ";value=" & CStr(x)
    F = x * 2
End Function
Public Function StateSummary() As String
    On Error GoTo Failed
    Dim s As String, r As Range, v As Variant
    s = "hold=" & CStr(holdCalls) & ";read=" & CStr(readCalls) & ";F=" & CStr(fCalls) & ";static-last=" & CStr(lastStatic)
    If held Is Nothing Then
        s = s & ";held=Nothing"
    Else
        s = s & ";parent=" & held.Parent.Parent.Name & ";address=" & held.Address(External:=True) & ";areas=" & CStr(held.Areas.Count) & ";type=" & TypeName(held)
        For Each r In held.Areas
            v = r.Value2
            s = s & ";area=" & r.Address(External:=True) & ";value-vt=" & CStr(VarType(v)) & ";value=" & CStr(v)
        Next
    End If
    StateSummary = s
    Exit Function
Failed:
    StateSummary = s & ";error=" & CStr(Err.Number) & ";" & Err.Description
End Function
Public Function ReadLog() As String
    Dim lines() As String, i As Long
    If used = 0 Then ReadLog = "<empty>": Exit Function
    ReDim lines(0 To used - 1)
    For i = 0 To used - 1
        lines(i) = logLines(i)
    Next
    ReadLog = Join(lines, vbLf)
End Function
Public Function HeldProperties() As String
    Dim key As Variant, v As Variant, n As Long, detail As String, valueText As String, result As String
    On Error Resume Next
    For Each key In Array("TypeName", "IsNothing", "ParentType", "ParentSheet", "ParentWorkbook", "AddressExternal", "Areas", "Value2")
        v = Empty
        Err.Clear
        Select Case CStr(key)
        Case "TypeName": v = TypeName(held)
        Case "IsNothing": v = (held Is Nothing)
        Case "ParentType": v = TypeName(held.Parent)
        Case "ParentSheet": v = held.Parent.Name
        Case "ParentWorkbook": v = held.Parent.Parent.Name
        Case "AddressExternal": v = held.Address(External:=True)
        Case "Areas": v = held.Areas.Count
        Case "Value2": v = held.Value2
        End Select
        n = Err.Number
        detail = Replace(Replace(Err.Description, vbCrLf, "<CRLF>"), vbLf, "<LF>")
        Err.Clear
        If IsArray(v) Then
            valueText = "<array>"
        ElseIf IsNull(v) Then
            valueText = "<Null>"
        Else
            valueText = CStr(v)
        End If
        If Len(result) > 0 Then result = result & ";"
        result = result & CStr(key) & "={vt=" & CStr(VarType(v)) & ";type=" & TypeName(v) & ";value=" & valueText & ";error=" & CStr(n) & ";" & detail & "}"
    Next
    HeldProperties = result
    On Error GoTo 0
End Function
