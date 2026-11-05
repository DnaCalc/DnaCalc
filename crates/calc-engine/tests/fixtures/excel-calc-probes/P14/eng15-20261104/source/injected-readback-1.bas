Option Explicit
Private trace() As String
Private used As Long
Private calls(0 To 100) As Long
Private currentStage As String
Private currentCase As String
Private observedSheet As Worksheet
Private eventSink As Object
Public editOnSave As Boolean
Private loggerReady As Boolean
Private Type DoubleBox
    value As Double
End Type
Private Type ByteBox
    value(0 To 7) As Byte
End Type

' MIT, DNA Calc engine12 questions. Combined with the existing calculation-common.bas.
' This file performs setup and in-memory observation only; it supplies no expected answers.

Public Sub Note(ByVal kind As String, ByVal detail As String)
    If Not loggerReady Then
        ReDim trace(0 To 200000)
        loggerReady = True
    End If
    If used > UBound(trace) Then Err.Raise 5, , "Trace capacity exceeded"
    trace(used) = currentCase & vbTab & currentStage & vbTab & kind & vbTab & detail
    used = used + 1
End Sub
Private Function Bits(ByVal x As Double) As String
    Dim d As DoubleBox, b As ByteBox, i As Long, t As String
    d.value = x
    LSet b = d
    For i = 7 To 0 Step -1
        t = t & right$("0" & hex$(b.value(i)), 2)
    Next
    Bits = t
End Function
Public Function Typed(ByVal x As Variant) As String
    Dim t As String
    If IsObject(x) Then
        Typed = "vt=9;type=" & TypeName(x) & ";value=<Object>"
        Exit Function
    End If
    t = "vt=" & CStr(VarType(x)) & ";type=" & TypeName(x) & ";value="
    If IsError(x) Then
        t = t & CStr(x)
    ElseIf IsEmpty(x) Then
        t = t & "<Empty>"
    ElseIf IsNull(x) Then
        t = t & "<Null>"
    ElseIf IsArray(x) Then
        t = t & "<Array>"
    Else
        t = t & Replace(Replace(CStr(x), vbTab, "<TAB>"), vbLf, "<LF>")
        If VarType(x) = vbDouble Then t = t & ";bits=" & Bits(CDbl(x))
    End If
    Typed = t
End Function
Public Sub Stage(ByVal label As String)
    currentStage = label
    Note "stage", label
End Sub
Public Sub Snap(ws As Worksheet, ByVal addresses As String)
    Dim a As Variant, r As Range, v As Variant
    For Each a In Split(addresses, ",")
        Set r = ws.Range(CStr(a))
        For Each r In r.Cells
            v = r.Value2
            Note "cell", ws.Name & "!" & r.address(False, False) & ";" & Typed(v)
        Next
    Next
End Sub
Public Sub CountSnapshot()
    Dim i As Long
    For i = 0 To UBound(calls)
        If calls(i) > 0 Then Note "counter", CStr(i) & "=" & CStr(calls(i))
    Next
End Sub
Public Function PlainProbe(ByVal ident As Long, x As Variant) As Variant
    calls(ident) = calls(ident) + 1
    Note "entry", "PlainProbe;id=" & CStr(ident) & ";n=" & CStr(calls(ident)) & ";arg=" & Typed(x) & ";caller=" & Application.Caller.address(External:=True)
    PlainProbe = x
    Note "exit", "PlainProbe;id=" & CStr(ident)
End Function
Public Function PeekB1() As Variant
    Dim v As Variant
    calls(1) = calls(1) + 1
    v = observedSheet.Range("B1").Value2
    Note "entry", "PeekB1;n=" & CStr(calls(1)) & ";caller=" & Application.Caller.address(External:=True) & ";read=" & Typed(v)
    PeekB1 = v
    Note "exit", "PeekB1"
End Function
Public Function PeekSelf() As Variant
    Dim v As Variant
    v = Application.Caller.Value2
    Note "entry", "PeekSelf;read=" & Typed(v)
    PeekSelf = v
    Note "exit", "PeekSelf"
End Function
Public Function PeekTwice() As Variant
    Dim a As Variant, b As Variant, s As Variant
    a = observedSheet.Range("B1").Value2
    Note "entry", "PeekTwice;first=" & Typed(a)
    s = Application.WorksheetFunction.Sum(1)
    b = observedSheet.Range("B1").Value2
    Note "read", "PeekTwice;second=" & Typed(b) & ";sum=" & Typed(s)
    PeekTwice = b
    Note "exit", "PeekTwice"
End Function
Public Sub Profile()
    Note "profile", "version=" & Application.Version & ";build=" & Application.Build & ";calculationVersion=" & CStr(Application.CalculationVersion) & ";os=" & Application.OperatingSystem
    Note "settings", "mode=" & CStr(Application.Calculation) & ";iteration=" & CStr(Application.Iteration) & ";maxIterations=" & CStr(Application.MaxIterations) & ";maxChange=" & CStr(Application.MaxChange) & ";calculateBeforeSave=" & CStr(Application.CalculateBeforeSave) & ";events=" & CStr(Application.EnableEvents) & ";precisionAsDisplayed=" & CStr(ThisWorkbook.PrecisionAsDisplayed) & ";date1904=" & CStr(ThisWorkbook.Date1904) & ";mtc=" & CStr(Application.MultiThreadedCalculation.Enabled) & ";threads=" & CStr(Application.MultiThreadedCalculation.ThreadCount) & ";automationSecurity=" & CStr(Application.AutomationSecurity)
    Note "force-full-setting", CStr(ThisWorkbook.ForceFullCalculation)
    Dim a As Object
    For Each a In Application.AddIns
        Note "addin", a.Name & ";installed=" & CStr(a.Installed)
    Next
    On Error GoTo ComAddInsUnavailable
    For Each a In Application.COMAddIns
        Note "com-addin", a.progID & ";connected=" & CStr(a.Connect)
    Next
    Exit Sub
ComAddInsUnavailable:
    Note "profile-unavailable", "COMAddIns;error=" & CStr(Err.Number) & ";" & Err.Description
    Err.Clear
End Sub
Public Sub SpillInfo(ws As Worksheet, ByVal atCell As String)
    On Error GoTo NoSpill
    Dim r As Range, flag As Variant
    Set r = ws.Range(atCell)
    flag = r.HasSpill
    Note "spill-flag", atCell & ";" & Typed(flag)
    If IsNull(flag) Then Exit Sub
    If Not CBool(flag) Then Exit Sub
    Note "spill", atCell & ";hasSpill=True;range=" & r.SpillingToRange.address
    Exit Sub
NoSpill:
    Note "spill", atCell & ";error=" & CStr(Err.Number) & ";" & Err.Description
    Err.Clear
End Sub
Public Function RunCase(ByVal ident As String, ByVal output As String) As String
    On Error GoTo Failed
    currentCase = ident
    ReDim trace(0 To 200000)
    loggerReady = True
    used = 0
    '__START_EVENT_SINK__
    If Not eventSink Is Nothing Then Set eventSink.XL = Application
    Application.Calculation = xlCalculationManual
    Application.CalculateBeforeSave = False
    Set observedSheet = ThisWorkbook.Worksheets(1)
    observedSheet.Name = "S"
    Stage "profile"
    Profile
    ProbeCase ident, output
    Stage "artifact-save"
    ThisWorkbook.SaveAs output, xlOpenXMLWorkbookMacroEnabled
    CountSnapshot
    Note "artifact", output
    GoTo Finished
Failed:
    Note "fault", CStr(Err.Number) & ";" & Err.Description
Finished:
    If used = 0 Then
        RunCase = "no observations"
    Else
        Dim resultLines() As String, k As Long
        ReDim resultLines(0 To used - 1)
        For k = 0 To used - 1
            resultLines(k) = trace(k)
        Next
        RunCase = Join(resultLines, vbLf)
    End If
End Function
Public Function ReadAfterOpen() As String
    currentCase = "reopened"
    Set observedSheet = ThisWorkbook.Worksheets(1)
    Stage "after-open"
    Snap observedSheet, "A1:C5,E1,E3:E8,G1:H3"
    Profile
    CountSnapshot
    Dim resultLines() As String, k As Long
    ReDim resultLines(0 To used - 1)
    For k = 0 To used - 1
        resultLines(k) = trace(k)
    Next
    ReadAfterOpen = Join(resultLines, vbLf)
End Function
Public Function RunProbe() As String
    On Error GoTo Failed
    If "0" = "1" Then
        RunProbe = "direct-command supervisor ready"
        Exit Function
    End If
    Dim i As Long, ident As Variant, wb As Workbook, component As Object, src As String, result As String, all As String, output As String, n As Long
    Application.Calculation = xlCalculationManual
    Application.CalculateBeforeSave = False
    Application.EnableEvents = True
    src = ThisWorkbook.VBProject.VBComponents("OracleProbe").CodeModule.Lines(1, ThisWorkbook.VBProject.VBComponents("OracleProbe").CodeModule.CountOfLines)
    For i = 1 To 1
        For Each ident In Split(SelectedCaseIds(), "|")
            Set wb = Application.Workbooks.Add(xlWBATWorksheet)
            wb.VBProject.VBComponents("ThisWorkbook").CodeModule.AddFromString EventSource()
            Set component = wb.VBProject.VBComponents.Add(2)
            component.Name = "ProbeEvents"
            component.CodeModule.AddFromString "Public WithEvents XL As Application" & vbCrLf & "Private Sub XL_AfterCalculate()" & vbCrLf & "OracleProbe.Note ""event"", ""AfterCalculate""" & vbCrLf & "End Sub"
            Set component = wb.VBProject.VBComponents.Add(1)
            component.Name = "OracleProbe"
            component.CodeModule.AddFromString Replace(src, "'__START_EVENT_SINK__", "Set eventSink = New ProbeEvents")
            output = "C:\Work\DnaCalc\codex-runs\excel-results-eng15\observer-preparation\eng13-observer-" & CStr(ident) & "-" & Format$(i, "00") & ".xlsm"
            result = Application.Run("'" & wb.Name & "'!OracleProbe.RunCase", CStr(ident) & "/" & CStr(i), output)
            all = all & result & vbLf
            wb.Close False
            Set wb = Nothing
        Next
    Next
    RunProbe = all
    Exit Function
Failed:
    RunProbe = all & vbLf & "driver-fault:" & CStr(Err.Number) & ":" & Err.Description
End Function
Public Function CreateOneCase(ByVal ident As String, ByVal output As String) As String
    Dim wb As Workbook, component As Object, src As String, ignored As String
    src = ThisWorkbook.VBProject.VBComponents("OracleProbe").CodeModule.Lines(1, ThisWorkbook.VBProject.VBComponents("OracleProbe").CodeModule.CountOfLines)
    Set wb = Application.Workbooks.Add(xlWBATWorksheet)
    wb.VBProject.VBComponents("ThisWorkbook").CodeModule.AddFromString EventSource()
    Set component = wb.VBProject.VBComponents.Add(2)
    component.Name = "ProbeEvents"
    component.CodeModule.AddFromString "Public WithEvents XL As Application" & vbCrLf & "Private Sub XL_AfterCalculate()" & vbCrLf & "OracleProbe.Note ""event"", ""AfterCalculate""" & vbCrLf & "End Sub"
    Set component = wb.VBProject.VBComponents.Add(1)
    component.Name = "OracleProbe"
    component.CodeModule.AddFromString Replace(src, "'__START_EVENT_SINK__", "Set eventSink = New ProbeEvents")
    ignored = Application.Run("'" & wb.Name & "'!OracleProbe.RunCase", ident, output)
    CreateOneCase = wb.Name & vbLf & ignored
End Function
Private Function SelectedCaseIds() As String
    SelectedCaseIds = ""
    SelectedCaseIds = SelectedCaseIds & "prepare~pair_explicit~5~3f50624dd2f1a9fc~forward~off|prepare~pair_hidden~5~3f50624dd2f1a9fc~forward~off"
    If Len(SelectedCaseIds) = 0 Then SelectedCaseIds = CaseIds()
End Function
Private Function EventSource() As String
    EventSource = "Option Explicit" & vbCrLf & _
        "Private Sub Workbook_SheetCalculate(ByVal Sh As Object)" & vbCrLf & _
        "OracleProbe.Note ""event"", ""SheetCalculate;sheet="" & Sh.Name" & vbCrLf & _
        "End Sub" & vbCrLf & _
        "Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)" & vbCrLf & _
        "OracleProbe.Note ""event"", ""BeforeSave;saveAsUI="" & CStr(SaveAsUI)" & vbCrLf & _
        "If OracleProbe.editOnSave Then" & vbCrLf & _
        "OracleProbe.editOnSave = False" & vbCrLf & _
        "Me.Worksheets(""S"").Range(""A1"").Value2 = 7" & vbCrLf & _
        "OracleProbe.Note ""event"", ""BeforeSave-edited-A1=7""" & vbCrLf & _
        "End If" & vbCrLf & _
        "End Sub" & vbCrLf & _
        "Private Sub Workbook_AfterSave(ByVal Success As Boolean)" & vbCrLf & _
        "OracleProbe.Note ""event"", ""AfterSave;success="" & CStr(Success)" & vbCrLf & _
        "End Sub"
End Function

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
    Note "entry", "PacketPairLog;n=" & CStr(calls(90)) & ";left=" & Typed(left) & ";right=" & Typed(right) & ";caller=" & Application.Caller.address(External:=True)
    PacketPairLog = 0#
    Note "exit", "PacketPairLog"
End Function
Public Function PacketPeekPair() As Double
    Dim a As Variant, b As Variant
    calls(91) = calls(91) + 1
    Note "entry", "PacketPeekPair;n=" & CStr(calls(91)) & ";caller=" & Application.Caller.address(External:=True)
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
    Note "entry", "PacketIndependent;n=" & CStr(calls(92)) & ";caller=" & Application.Caller.address(External:=True)
    PacketIndependent = 0#
    Note "exit", "PacketIndependent"
End Function

