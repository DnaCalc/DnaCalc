Attribute VB_Name = "Probe"
Option Explicit
' Volatility probes (vol-1). Every test function logs one entry per call under "<name>@<caller>" and
' returns 1; Cnt(x) is the dependent counter (it reads x, so it is calculated whenever x was).
' Script characters (same language as the XLL fixture's volatility.rs): T = Application.Volatile True,
' F = Application.Volatile False, N = Application.Volatile with no argument; "|" separates the parts
' of a script, call k of a name runs part k and later calls repeat the last part; a leading "*" cycles.
' VCfg(name, script) overrides a name's script, VReset clears the counts GetCounts lists, VResetCalls also
' restarts the per-name call numbers.
Private keys(0 To 999) As String
Private counts(0 To 999) As Long
Private used As Long
Private callNo(0 To 99) As Long
Private callName(0 To 99) As String
Private callUsed As Long
Private cfgName(0 To 99) As String
Private cfgScript(0 To 99) As String
Private cfgUsed As Long
Private status As String
Private total As Long

Private Function CallerAddr() As String
    On Error Resume Next
    CallerAddr = Application.Caller.Address(False, False)
    If Err.Number <> 0 Then CallerAddr = "nocaller"
End Function

Private Sub Bump(ByVal key As String)
    Dim i As Long
    For i = 0 To used - 1
        If keys(i) = key Then counts(i) = counts(i) + 1: Exit Sub
    Next i
    If used < 1000 Then keys(used) = key: counts(used) = 1: used = used + 1
End Sub

Private Function NextCall(ByVal nm As String) As Long
    Dim i As Long
    For i = 0 To callUsed - 1
        If callName(i) = nm Then callNo(i) = callNo(i) + 1: NextCall = callNo(i): Exit Function
    Next i
    callName(callUsed) = nm: callNo(callUsed) = 1: callUsed = callUsed + 1: NextCall = 1
End Function

Private Function ScriptFor(ByVal nm As String, ByVal dflt As String) As String
    Dim i As Long
    For i = 0 To cfgUsed - 1
        If cfgName(i) = nm Then ScriptFor = cfgScript(i): Exit Function
    Next i
    ScriptFor = dflt
End Function

Private Function Run1(ByVal nm As String, ByVal dflt As String) As Variant
    Dim n As Long, s As String, parts() As String, part As String, k As Long, ch As String
    n = NextCall(nm)
    Bump nm & "@" & CallerAddr()
    s = ScriptFor(nm, dflt)
    Dim cyc As Boolean
    If Left$(s, 1) = "*" Then cyc = True: s = Mid$(s, 2)
    parts = Split(s, "|")
    If UBound(parts) < 0 Then ReDim parts(0 To 0)
    If cyc Then
        k = (n - 1) Mod (UBound(parts) + 1)
    Else
        k = n - 1
        If k > UBound(parts) Then k = UBound(parts)
    End If
    part = parts(k)
    Dim i As Long
    For i = 1 To Len(part)
        ch = Mid$(part, i, 1)
        If ch = "T" Then
            Application.Volatile True
        ElseIf ch = "F" Then
            Application.Volatile False
        ElseIf ch = "N" Then
            Application.Volatile
        End If
    Next i
    Run1 = 1
End Function

Public Function Cnt(ByVal x As Variant, Optional ByVal y As Variant) As Variant
    Bump "Cnt@" & CallerAddr()
    total = total + 1
    Cnt = total
End Function
Public Function CntR(ByRef x As Variant) As Variant
    Bump "Cnt@" & CallerAddr()
    CntR = 0
End Function
Public Function VPlain() As Variant: VPlain = Run1("VPlain", ""): End Function
Public Function VTrue() As Variant: VTrue = Run1("VTrue", "T"): End Function
Public Function VFalse() As Variant: VFalse = Run1("VFalse", "F"): End Function
Public Function VTF() As Variant: VTF = Run1("VTF", "TF"): End Function
Public Function VFT() As Variant: VFT = Run1("VFT", "FT"): End Function
Public Function VTTF() As Variant: VTTF = Run1("VTTF", "TTF"): End Function
Public Function VSeq() As Variant: VSeq = Run1("VSeq", "*T|F"): End Function
Public Function VArgF(ByVal x As Variant) As Variant: VArgF = Run1("VArgF", "F"): End Function
Public Function VArgT(ByVal x As Variant) As Variant: VArgT = Run1("VArgT", "T"): End Function
Public Function VArgP(ByVal x As Variant) As Variant: VArgP = Run1("VArgP", ""): End Function
Public Function VAtOne() As Variant: VAtOne = Run1("VAtOne", "T|"): End Function
Public Function VAtTwo() As Variant: VAtTwo = Run1("VAtTwo", "|T|"): End Function
Public Function VAtThree() As Variant: VAtThree = Run1("VAtThree", "||T|"): End Function
Public Function VAtFour() As Variant: VAtFour = Run1("VAtFour", "|||T|"): End Function
Public Function VAtTwoB() As Variant: VAtTwoB = Run1("VAtTwoB", "|T|"): End Function
Public Function VAtFive() As Variant: VAtFive = Run1("VAtFive", "||||T|"): End Function
Public Function VElemD(ByVal x As Double) As Variant
    Bump "VElemD@" & CallerAddr()
    If x > 1 Then Application.Volatile True Else Application.Volatile False
    VElemD = x
End Function
Public Function VNoArg() As Variant: VNoArg = Run1("VNoArg", "N"): End Function
Public Function VOnceTrue() As Variant: VOnceTrue = Run1("VOnceTrue", "T|"): End Function
Public Function VOnceFalse() As Variant: VOnceFalse = Run1("VOnceFalse", "F|"): End Function
' With an argument: the request may depend on the element (X2): VElem(x) requests True for x > 0.
Public Function VArg(ByVal x As Variant) As Variant
    Bump "VArg@" & CallerAddr()
    VArg = 1
End Function
Public Function VElem(ByVal x As Variant) As Variant
    Bump "VElem@" & CallerAddr()
    If IsNumeric(x) Then
        If x > 0 Then Application.Volatile True Else Application.Volatile False
    End If
    VElem = x
End Function
' Calls another VBA function that requests; the helper's request counts for the caller cell.
Public Function VHelper() As Variant
    Bump "VHelper@" & CallerAddr()
    Dim d As Variant
    d = VTrue()
    VHelper = 1
End Function
' Reads cell B1 (a producer) before the request; used by the abandoned-attempt probe.
Public Function VFirstOnly(ByVal x As Variant) As Variant
    Dim n As Long
    n = NextCall("VFirstOnly")
    Bump "VFirstOnly@" & CallerAddr()
    If n = 1 Then Application.Volatile True
    VFirstOnly = 1
End Function

Public Sub VCfg(ByVal nm As String, ByVal script As String)
    Dim i As Long
    For i = 0 To cfgUsed - 1
        If cfgName(i) = nm Then cfgScript(i) = script: Exit Sub
    Next i
    cfgName(cfgUsed) = nm: cfgScript(cfgUsed) = script: cfgUsed = cfgUsed + 1
End Sub

Public Sub VReset()
    used = 0
End Sub

Public Sub VResetCalls()
    used = 0
    callUsed = 0
    status = ""
End Sub

Public Function GetCounts() As String
    Dim i As Long, s As String
    For i = 0 To used - 1
        s = s & keys(i) & "=" & counts(i) & ";"
    Next i
    GetCounts = s
End Function

' Calls made outside a worksheet formula (X5): each returns the status text.
Public Function OutsideVolatile() As String
    On Error Resume Next
    Application.Volatile True
    OutsideVolatile = "err=" & Err.Number & " " & Err.Description
End Function
Public Sub SubVolatile()
    On Error Resume Next
    Application.Volatile True
    status = status & "sub:err=" & Err.Number & ";"
End Sub
Public Function GetStatus() As String
    GetStatus = status
End Function
