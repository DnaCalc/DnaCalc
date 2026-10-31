Private Declare PtrSafe Function DumpNative Lib "__NATIVE__" (ByVal path As LongPtr) As Long
Private Declare PtrSafe Function ResetNative Lib "__NATIVE__" () As Long
Private Declare PtrSafe Function MarkNative Lib "__NATIVE__" (ByVal stageNumber As Long) As Long
Private nativeArmed As Boolean
Private Sub NativeStage(ByVal label As String, ByVal stageNumber As Long)
    Stage label
    Dim n As Long
    If nativeArmed Then n = MarkNative(stageNumber)
End Sub
Private Function CaseIds() As String
    CaseIds = "range-direct|variant-direct|variant-expression|double-direct|double-expression|unused-direct|xll-ordinary|xll-macro|xll-macro-nonvolatile|xll-threadsafe|xll-inside"
End Function
Public Function LateRange(x As Range) As Variant
    Dim v As Variant, hidden As Variant
    calls(41) = calls(41) + 1
    v = x.Value2
    hidden = observedSheet.Range("Z1").Value2
    Note "entry", "LateRange;n=" & CStr(calls(41)) & ";argument=" & Typed(v) & ";hidden=" & Typed(hidden)
    LateRange = v
    Note "exit", "LateRange"
End Function
Public Function LateVariant(x As Variant) As Variant
    Dim v As Variant
    calls(42) = calls(42) + 1
    If IsObject(x) Then v = x.Value2 Else v = x
    Note "entry", "LateVariant;n=" & CStr(calls(42)) & ";argument=" & Typed(x) & ";value=" & Typed(v) & ";hidden=" & Typed(observedSheet.Range("Z1").Value2)
    LateVariant = v
    Note "exit", "LateVariant"
End Function
Public Function LateDouble(ByVal x As Double) As Double
    calls(43) = calls(43) + 1
    Note "entry", "LateDouble;n=" & CStr(calls(43)) & ";argument=" & Typed(x) & ";hidden=" & Typed(observedSheet.Range("Z1").Value2)
    LateDouble = x
    Note "exit", "LateDouble"
End Function
Public Function UnusedArg(x As Variant) As Long
    calls(44) = calls(44) + 1
    Note "entry", "UnusedArg;n=" & CStr(calls(44)) & ";argument-vt=" & CStr(VarType(x)) & ";argument-type=" & TypeName(x)
    UnusedArg = 44
    Note "exit", "UnusedArg"
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim ws As Worksheet, p As String, f As String, n As Long, path As String
    Set ws = observedSheet
    p = Split(ident, "/")(0)
    Stage "setup"
    ws.Range("A1").Value2 = 1
    ws.Range("Z1").Formula2 = "=A1+100"
    If Left$(p, 3) = "xll" Then
        Note "register-xll", CStr(Application.RegisterXLL("__NATIVE__"))
        n = ResetNative()
        nativeArmed = True
        Select Case p
        Case "xll-ordinary": f = "=XOrd(B1)"
        Case "xll-macro": f = "=XMacro(B1)"
        Case "xll-macro-nonvolatile": f = "=XMacroNV(B1)"
        Case "xll-threadsafe": f = "=XThread(B1)"
        Case "xll-inside": f = "=XInside()"
        End Select
    Else
        Select Case p
        Case "range-direct": f = "=LateRange(B1)"
        Case "variant-direct": f = "=LateVariant(B1)"
        Case "variant-expression": f = "=LateVariant(B1+0)"
        Case "double-direct": f = "=LateDouble(B1)"
        Case "double-expression": f = "=LateDouble(B1+0)"
        Case "unused-direct": f = "=UnusedArg(B1)"
        End Select
    End If
    NativeStage "entry-consumer-before-producer", 1
    ws.Range("C1").Formula2 = f
    ws.Range("B1").Formula2 = "=PlainProbe(31,A1*0)+11"
    NativeStage "first-calculation", 2
    Application.Calculate
    Snap ws, "A1:C1,Z1"
    CountSnapshot
    NativeStage "warm-f9", 3
    Application.Calculate
    Snap ws, "B1:C1"
    CountSnapshot
    NativeStage "full", 4
    Application.CalculateFull
    CountSnapshot
    NativeStage "rebuild", 5
    Application.CalculateFullRebuild
    CountSnapshot
    NativeStage "same-value-rewrite", 6
    ws.Range("A1").Value2 = 1
    Application.Calculate
    Snap ws, "B1:C1,Z1"
    CountSnapshot
    NativeStage "upstream-change-equal-result", 7
    ws.Range("A1").Value2 = 2
    Application.Calculate
    Snap ws, "B1:C1,Z1"
    CountSnapshot
    NativeStage "dirty-producer-consumer-first", 8
    ws.Range("A1").Value2 = 3
    ws.Range("C1,B1").Calculate
    Snap ws, "B1:C1,Z1"
    CountSnapshot
    If Left$(p, 3) = "xll" Then
        path = Replace(output, ".xlsm", "-native.tsv")
        n = DumpNative(StrPtr(path))
        Note "native-records", CStr(n)
    End If
End Sub
