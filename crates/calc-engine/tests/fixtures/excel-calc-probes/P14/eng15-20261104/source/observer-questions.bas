' MIT, DNA Calc engine12 questions. Combined with the existing calculation-common.bas.
' This file performs setup and in-memory observation only; it supplies no expected answers.
Private Function CaseIds() As String
    CaseIds = ""
End Function
Private Function PacketFormula(ByVal kind As String, ByVal address As String) As String
    Select Case kind
    Case "pure_scc"
        Select Case address
        Case "A1": PacketFormula = "=A1/2+1"
        Case "B1": PacketFormula = "=A1+0"
        Case "C1": PacketFormula = "=C1+1"
        Case "D1": PacketFormula = "=C1+0"
        End Select
    Case "pure_convergent_only"
        Select Case address
        Case "A1": PacketFormula = "=A1/2+1"
        Case "B1": PacketFormula = "=A1+0"
        Case "C1": PacketFormula = "=0"
        Case "D1": PacketFormula = "=0"
        End Select
    Case "scc_counts"
        Select Case address
        Case "A1": PacketFormula = "=PlainProbe(1,A1/2+1)"
        Case "B1": PacketFormula = "=PlainProbe(3,A1+0)"
        Case "C1": PacketFormula = "=PlainProbe(2,C1+1)"
        Case "D1": PacketFormula = "=PlainProbe(4,C1+0)"
        End Select
    Case "scc_convergent_only"
        Select Case address
        Case "A1": PacketFormula = "=PlainProbe(1,A1/2+1)"
        Case "B1": PacketFormula = "=PlainProbe(3,A1+0)"
        Case "C1": PacketFormula = "=0"
        Case "D1": PacketFormula = "=0"
        End Select
    Case "scc_counts_volatile"
        Select Case address
        Case "A1": PacketFormula = "=PlainProbe(1,A1/2+1)"
        Case "B1": PacketFormula = "=PlainProbe(3,A1+0)"
        Case "C1": PacketFormula = "=PlainProbe(2,C1+1)"
        Case "D1": PacketFormula = "=PlainProbe(4,C1+0)"
        Case "E1": PacketFormula = "=PacketIndependent()"
        End Select
    Case "pure_pair_explicit"
        Select Case address
        Case "A1": PacketFormula = "=B1+1"
        Case "B1": PacketFormula = "=A1+1"
        Case "C1": PacketFormula = "=A1+B1"
        Case "D1": PacketFormula = "=0*(A1+B1)"
        End Select
    Case "pure_pair_hidden"
        Select Case address
        Case "A1": PacketFormula = "=B1+1"
        Case "B1": PacketFormula = "=A1+1"
        Case "C1": PacketFormula = "=A1+B1"
        Case "D1": PacketFormula = "=0"
        End Select
    Case "pair_explicit"
        Select Case address
        Case "A1": PacketFormula = "=B1+1"
        Case "B1": PacketFormula = "=A1+1"
        Case "C1": PacketFormula = "=A1+B1"
        Case "D1": PacketFormula = "=PacketPairLog(A1,B1)"
        End Select
    Case "pair_hidden"
        Select Case address
        Case "A1": PacketFormula = "=B1+1"
        Case "B1": PacketFormula = "=A1+1"
        Case "C1": PacketFormula = "=A1+B1"
        Case "D1": PacketFormula = "=PacketPeekPair()"
        End Select
    Case Else
        Err.Raise 5, , "Unknown observer kind"
    End Select
End Function
Private Function PacketDouble(ByVal hex As String) As Double
    Dim d As DoubleBox, b As ByteBox, i As Long
    If Len(hex) <> 16 Then Err.Raise 5, , "Bad binary64 setting"
    For i = 0 To 7
        b.value(i) = CByte("&H" & Mid$(hex, 15 - 2 * i, 2))
    Next
    LSet d = b
    PacketDouble = d.value
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim p As Variant, addresses As Variant, address As Variant, f As String
    p = Split(Split(ident, "/")(0), "~")
    If UBound(p) <> 5 Then Err.Raise 5, , "Expected a preparation identity"
    If p(0) <> "prepare" Then Err.Raise 5, , "This entry prepares only; observation commands belong to the external controller"
    If p(4) <> "forward" And p(4) <> "reverse" Then Err.Raise 5, , "Unknown entry order"
    If p(5) <> "off" And p(5) <> "on" Then Err.Raise 5, , "Unknown MTC setting"
    Application.Calculation = xlCalculationManual
    Application.EnableEvents = False
    Application.Iteration = True
    Application.MaxIterations = CLng(p(2))
    Application.MaxChange = PacketDouble(CStr(p(3)))
    If LCase$(Bits(Application.MaxChange)) <> LCase$(CStr(p(3))) Then Err.Raise 5, , "MaxChange readback differs"
    Application.MultiThreadedCalculation.Enabled = (p(5) = "on")
    Set observedSheet = ThisWorkbook.Worksheets(1)
    observedSheet.Name = "S"
    Stage "prepare-formulas-not-a-measured-command"
    If p(4) = "reverse" Then
        addresses = Array("E1", "D1", "C1", "B1", "A1")
    Else
        addresses = Array("A1", "B1", "C1", "D1", "E1")
    End If
    For Each address In addresses
        f = PacketFormula(CStr(p(1)), CStr(address))
        If Len(f) > 0 Then observedSheet.Range(CStr(address)).Formula2 = f
    Next
    Profile
    Snap observedSheet, "A1:E1"
    Note "question", "Setup may have calculated; close, patch caches with Set-StoredValues, then observe an independently opened copy"
End Sub
Public Function PacketArm(ByVal ident As String) As String
    Dim i As Long
    For i = 0 To UBound(calls)
        If calls(i) <> 0 Then Err.Raise 5, , "A UDF ran before observation was armed; retain the startup trace and abort this trial"
    Next
    currentCase = ident
    Set observedSheet = ThisWorkbook.Worksheets("S")
    ReDim trace(0 To 200000)
    loggerReady = True
    used = 0
    Stage "armed-after-verified-seeds"
    Profile
    PacketArm = "armed"
End Function
Public Sub PacketStage(ByVal label As String)
    Stage label
End Sub
Public Function PacketPairLog(ByVal left As Double, ByVal right As Double) As Double
    calls(90) = calls(90) + 1
    Note "entry", "PacketPairLog;n=" & CStr(calls(90)) & ";left=" & Typed(left) & ";right=" & Typed(right) & ";caller=" & Application.Caller.Address(External:=True)
    PacketPairLog = 0#
    Note "exit", "PacketPairLog"
End Function
Public Function PacketPeekPair() As Double
    Dim a As Variant, b As Variant
    calls(91) = calls(91) + 1
    Note "entry", "PacketPeekPair;n=" & CStr(calls(91)) & ";caller=" & Application.Caller.Address(External:=True)
    a = observedSheet.Range("A1").Value2
    b = observedSheet.Range("B1").Value2
    Note "read", "PacketPeekPair;first;left=" & Typed(a) & ";right=" & Typed(b)
    a = observedSheet.Range("A1").Value2
    b = observedSheet.Range("B1").Value2
    Note "read", "PacketPeekPair;second;left=" & Typed(a) & ";right=" & Typed(b)
    PacketPeekPair = 0#
    Note "exit", "PacketPeekPair"
End Function
Public Function PacketIndependent() As Double
    Application.Volatile True
    calls(92) = calls(92) + 1
    Note "entry", "PacketIndependent;n=" & CStr(calls(92)) & ";caller=" & Application.Caller.Address(External:=True)
    PacketIndependent = 0#
    Note "exit", "PacketIndependent"
End Function
