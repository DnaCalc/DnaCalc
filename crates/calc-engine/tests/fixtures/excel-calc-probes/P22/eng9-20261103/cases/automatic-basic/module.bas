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

Private appSink As P22Events
Private changeEdit As Boolean, calcEdit As Boolean, cancelSave As Boolean
Private changedOnce As Boolean, calculatedOnce As Boolean, savedOnce As Boolean

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
        t = t & Right$("0" & Hex$(b.value(i)), 2)
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
    Note "spill", atCell & ";hasSpill=True;range=" & r.SpillingToRange.Address
    Exit Sub
NoSpill:
    Note "spill", atCell & ";error=" & CStr(Err.Number) & ";" & Err.Description
    Err.Clear
End Sub
Public Sub Initialize(ByVal ident As String, ByVal changeFlag As Boolean, ByVal calcFlag As Boolean, ByVal saveFlag As Boolean, ByVal cancelFlag As Boolean)
    currentCase = ident
    currentStage = "initialize"
    ReDim trace(0 To 200000)
    used = 0
    loggerReady = True
    Set observedSheet = ThisWorkbook.Worksheets("S")
    changeEdit = changeFlag
    calcEdit = calcFlag
    editOnSave = saveFlag
    cancelSave = cancelFlag
    changedOnce = False
    calculatedOnce = False
    savedOnce = False
    Set appSink = New P22Events
    Set appSink.App = Application
    Profile
End Sub
Public Sub Observe(ByVal kind As String, ByVal detail As String)
    Note kind, detail
    Snap observedSheet, "A1:D1"
End Sub
Public Sub WorksheetChange(ByVal target As Range)
    Observe "Worksheet_Change entry", target.Address(False, False)
    If changeEdit And Not changedOnce Then
        changedOnce = True
        observedSheet.Range("D1").Value2 = observedSheet.Range("A1").Value2 + 100
        Observe "Worksheet_Change edit returned", "D1"
    End If
    Observe "Worksheet_Change exit", target.Address(False, False)
End Sub
Public Sub WorksheetCalculate()
    Observe "Worksheet_Calculate entry", ""
    If calcEdit And Not calculatedOnce Then
        calculatedOnce = True
        observedSheet.Range("A1").Value2 = 7
        Observe "Worksheet_Calculate edit returned", "A1"
    End If
    Observe "Worksheet_Calculate exit", ""
End Sub
Public Sub BeforeSave(ByVal source As String, ByVal saveAsUI As Boolean, ByRef cancel As Boolean)
    Observe source & " BeforeSave entry", "SaveAsUI=" & Typed(saveAsUI) & ";Cancel=" & Typed(cancel)
    If source = "Workbook" Then
        If editOnSave And Not savedOnce Then
            savedOnce = True
            observedSheet.Range("A1").Value2 = 9
            Observe "BeforeSave edit returned", "A1"
        End If
        If cancelSave Then cancel = True
    End If
    Observe source & " BeforeSave exit", "SaveAsUI=" & Typed(saveAsUI) & ";Cancel=" & Typed(cancel)
End Sub
Public Function ReadLog() As String
    Dim lines() As String, i As Long
    If used = 0 Then Exit Function
    ReDim lines(0 To used - 1)
    For i = 0 To used - 1
        lines(i) = trace(i)
    Next
    ReadLog = Join(lines, vbLf)
End Function
Public Sub Shutdown()
    If Not appSink Is Nothing Then Set appSink.App = Nothing
    Set appSink = Nothing
End Sub