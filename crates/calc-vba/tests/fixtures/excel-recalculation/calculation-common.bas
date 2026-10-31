Option Explicit
Private trace() As String
Private used As Long
Private calls(0 To 100) As Long
Private currentStage As String
Private currentCase As String
Private observedSheet As Worksheet
Private eventSink As Object
Public editOnSave As Boolean
Private Type DoubleBox
    value As Double
End Type
Private Type ByteBox
    value(0 To 7) As Byte
End Type

Public Sub Note(ByVal kind As String, ByVal detail As String)
    If used > UBound(trace) Then Err.Raise 5, , "Trace capacity exceeded"
    trace(used) = currentCase & vbTab & currentStage & vbTab & kind & vbTab & detail
    used = used + 1
End Sub
Private Function Bits(ByVal x As Double) As String
    Dim d As DoubleBox, b As ByteBox, i As Long, t As String
    d.value = x
    LSet b = d
    For i = 7 To 0 Step -1
        t = t & Right$("0" & Hex$(b.value(i)), 2)
    Next
    Bits = t
End Function
Public Function Typed(ByVal x As Variant) As String
    Dim t As String
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
            Note "cell", ws.Name & "!" & r.Address(False, False) & ";" & Typed(v)
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
    Note "entry", "PlainProbe;id=" & CStr(ident) & ";n=" & CStr(calls(ident)) & ";arg=" & Typed(x) & ";caller=" & Application.Caller.Address(External:=True)
    PlainProbe = x
    Note "exit", "PlainProbe;id=" & CStr(ident)
End Function
Public Function PeekB1() As Variant
    Dim v As Variant
    calls(1) = calls(1) + 1
    v = observedSheet.Range("B1").Value2
    Note "entry", "PeekB1;n=" & CStr(calls(1)) & ";caller=" & Application.Caller.Address(External:=True) & ";read=" & Typed(v)
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
    Dim r As Range
    Set r = ws.Range(atCell)
    Note "spill", atCell & ";hasSpill=" & CStr(r.HasSpill) & ";range=" & r.SpillingToRange.Address
    Exit Sub
NoSpill:
    Note "spill", atCell & ";error=" & CStr(Err.Number) & ";" & Err.Description
    Err.Clear
End Sub
Public Function RunCase(ByVal ident As String, ByVal output As String) As String
    On Error GoTo Failed
    currentCase = ident
    ReDim trace(0 To 200000)
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
Public Function RunProbe() As String
    On Error GoTo Failed
    Dim i As Long, ident As Variant, wb As Workbook, component As Object, src As String, result As String, all As String, output As String, n As Long
    Application.Calculation = xlCalculationManual
    Application.CalculateBeforeSave = False
    Application.EnableEvents = True
    src = ThisWorkbook.VBProject.VBComponents("OracleProbe").CodeModule.Lines(1, ThisWorkbook.VBProject.VBComponents("OracleProbe").CodeModule.CountOfLines)
    For i = 1 To __REPETITIONS__
        For Each ident In Split(CaseIds(), "|")
            Set wb = Application.Workbooks.Add(xlWBATWorksheet)
            wb.VBProject.VBComponents("ThisWorkbook").CodeModule.AddFromString EventSource()
            Set component = wb.VBProject.VBComponents.Add(2)
            component.Name = "ProbeEvents"
            component.CodeModule.AddFromString "Public WithEvents XL As Application" & vbCrLf & "Private Sub XL_AfterCalculate()" & vbCrLf & "OracleProbe.Note ""event"", ""AfterCalculate""" & vbCrLf & "End Sub"
            Set component = wb.VBProject.VBComponents.Add(1)
            component.Name = "OracleProbe"
            component.CodeModule.AddFromString Replace(src, "'__START_EVENT_SINK__", "Set eventSink = New ProbeEvents")
            output = "__OUTPUT__\" & CStr(ident) & "-" & Format$(i, "00") & ".xlsm"
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
