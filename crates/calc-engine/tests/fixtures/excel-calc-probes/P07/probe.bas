Private Declare PtrSafe Function DumpNative Lib "__NATIVE__" (ByVal path As LongPtr) As Long
Private Declare PtrSafe Function ResetNative Lib "__NATIVE__" () As Long
Private Declare PtrSafe Function MarkNative Lib "__NATIVE__" (ByVal stageNumber As Long) As Long
Private Declare PtrSafe Function ReleaseAfter Lib "__NATIVE__" (ByVal milliseconds As Long) As Long
Private Declare PtrSafe Function ReleaseLatchAfter Lib "__NATIVE__" (ByVal latch As Long, ByVal milliseconds As Long) As Long
Private Declare PtrSafe Function ReleaseCalls Lib "__NATIVE__" (ByVal newestFirst As Long) As Long
Private Declare PtrSafe Function PendingAsync Lib "__NATIVE__" () As Long
Private Declare PtrSafe Function UnregisterAsync Lib "__NATIVE__" () As Long
Private Declare PtrSafe Function RegisterAsync Lib "__NATIVE__" () As Long
Private Declare PtrSafe Function GetCurrentProcessId Lib "kernel32" () As Long
Private Declare PtrSafe Sub Sleep Lib "kernel32" (ByVal milliseconds As Long)
Private directOwner As Workbook
Private directTarget As String
Private directOutput As String
Private directControl As String
Private Function CaseIds() As String
    CaseIds = "normal|nested|overwrite|undo|arg-edit|move|close-reopen|duplicate|two-sites|supersede|reverse-release|reload|save-pending"
End Function
Public Function ReloadAsync() As String
    Dim status As Long
    status = UnregisterAsync()
    ReloadAsync = "unregister=" & CStr(status)
    If status = 0 Then ReloadAsync = ReloadAsync & ";register=" & CStr(RegisterAsync())
    Note "provider-reload", ReloadAsync
End Function
Private Sub WaitController(ByVal control As String)
    Dim start As Single
    start = Timer
    Do While Len(Dir$(control & ".done")) = 0 And Timer - start < 12
        DoEvents
        Sleep 10
    Loop
    Note "controller-done", CStr(Len(Dir$(control & ".done")) > 0)
End Sub
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim owner As Workbook, ws As Worksheet, wb As Workbook, p As String, op As String, formula As String
    Dim latch As Long, secondLatch As Long, n As Long, path As String, target As String, control As String, f As Integer, start As Single
    p = Split(ident, "/")(0)
    Stage "register-and-setup"
    Note "register-xll", CStr(Application.RegisterXLL("__NATIVE__"))
    n = ResetNative()
    Set owner = Application.Workbooks.Add(xlWBATWorksheet)
    Set ws = owner.Worksheets(1)
    ws.Name = "AsyncTarget"
    target = Replace(output, ".xlsm", "-target.xlsx")
    latch = CLng(Timer * 1000)
    ws.Range("A1").Value2 = 41
    ws.Range("B1").Value2 = latch
    formula = "=XBefore(1)+AsyncEcho(A1,B1)+XAfter(2)"
    If p = "nested" Then formula = "=XAfter(AsyncEcho(XBefore(A1),B1))"
    If p = "two-sites" Then formula = "=AsyncEcho(A1,B1)+AsyncEcho(A1+1,B1+1)"
    If p = "duplicate" Then ws.Range("B1").Value2 = -latch
    n = ReleaseAfter(250)
    If p = "two-sites" Then
        n = ReleaseAfter(750)
        n = ReleaseAfter(1500)
        n = ReleaseAfter(2500)
    End If
    ws.Range("C1").Formula2 = formula
    ws.Range("D1").Formula2 = "=C1+1"
    Stage "seed-calculation"
    n = ReleaseAfter(500)
    Application.Calculate
    start = Timer
    Do While Timer - start < IIf(p = "two-sites", 2.7, 0.7)
        DoEvents
        Sleep 10
    Loop
    Snap ws, "A1:D1"
    If IsError(ws.Range("C1").Value2) Then
        Note "unrunnable", "Async seed is an error; no pending-mutation inference"
        GoTo FinishCase
    End If
    owner.SaveAs target, xlOpenXMLWorkbook
    n = ResetNative()
    Stage "dirty-argument"
    ws.Range("A1").Value2 = 42
    latch = latch + 100
    ws.Range("B1").Value2 = latch
    If p = "duplicate" Then ws.Range("B1").Value2 = -latch
    secondLatch = 0
    If p = "two-sites" Or p = "reverse-release" Then secondLatch = latch + 1
    op = p
    If p = "nested" Or p = "duplicate" Or p = "two-sites" Then op = "normal"
    control = "__OUTPUT__\external-control-__RUN_ID__-" & CStr(GetCurrentProcessId()) & "-" & CStr(latch) & ".txt"
    f = FreeFile
    Open control For Output As #f
    Print #f, CStr(GetCurrentProcessId()) & vbTab & owner.Name & vbTab & op & vbTab & CStr(latch) & vbTab & CStr(secondLatch) & vbTab & target & vbTab & ThisWorkbook.Name
    Close #f
    n = ReleaseLatchAfter(latch, 5000)
    If secondLatch <> 0 Then n = ReleaseLatchAfter(secondLatch, 5000)
    If "__EXTERNAL_DIRECT__" = "1" Then
        Set directOwner = owner
        directTarget = target
        directOutput = output
        directControl = control
        ThisWorkbook.Worksheets(1).Range("A1").Value2 = owner.Name
        ThisWorkbook.Worksheets(1).Range("A2").Value2 = target
        ThisWorkbook.Worksheets(1).Range("A3").Value2 = output
        ThisWorkbook.Worksheets(1).Range("A4").Value2 = control
        ThisWorkbook.Worksheets(1).Range("A5").Value2 = ident
        Note "prepared-direct-command", owner.Name
        Exit Sub
    End If
    Stage "tested-calculation"
    n = MarkNative(1)
    start = Timer
    On Error GoTo CalculationReturnedError
    ws.Range("C1").Calculate
    Note "calculation-return", "duration-seconds=" & CStr(Timer - start) & ";state=" & CStr(Application.CalculationState)
    GoTo AwaitController
CalculationReturnedError:
    Note "calculation-error", CStr(Err.Number) & ";" & Err.Description & ";duration-seconds=" & CStr(Timer - start)
    Err.Clear
    Resume AwaitController
AwaitController:
    On Error GoTo 0
    Stage "await-external-controller"
    WaitController control
    start = Timer
    Do While Timer - start < 0.3
        DoEvents
        Sleep 10
    Loop
FinishCase:
    n = ReleaseCalls(0)
    start = Timer
    Do While PendingAsync() > 0 And Timer - start < 10
        DoEvents
        Sleep 10
    Loop
    Note "pending-native-callbacks", CStr(PendingAsync())
    If PendingAsync() > 0 Then Note "unrunnable", "Native callbacks have not drained; native tail is incomplete"
    Stage "after"
    For Each wb In Application.Workbooks
        If wb.FullName = target Or wb Is owner Then
            Snap wb.Worksheets(1), "A1:F1"
            Note "calculation-state", CStr(Application.CalculationState)
            Application.CalculateBeforeSave = False
            wb.SaveCopyAs Replace(output, ".xlsm", "-after.xlsx")
            Exit For
        End If
    Next
    path = Replace(output, ".xlsm", "-native.tsv")
    n = DumpNative(StrPtr(path))
    Note "native-records", CStr(n)
    On Error Resume Next
    For Each wb In Application.Workbooks
        If wb.FullName = target Or wb Is owner Then wb.Close False
    Next
    On Error GoTo 0
End Sub
Public Function CompleteDirectCase() As String
    Dim start As Single, n As Long, wb As Workbook, path As String
    LoadDirectState
    Stage "direct-com-returned"
    WaitController directControl
    n = ReleaseCalls(0)
    start = Timer
    Do While PendingAsync() > 0 And Timer - start < 10
        DoEvents
        Sleep 10
    Loop
    Note "pending-native-callbacks", CStr(PendingAsync())
    If PendingAsync() > 0 Then Note "unrunnable", "Native callbacks did not drain"
    Stage "after"
    For Each wb In Application.Workbooks
        If wb.FullName = directTarget Or wb Is directOwner Then
            Snap wb.Worksheets(1), "A1:F1"
            Note "calculation-state", CStr(Application.CalculationState)
            wb.SaveCopyAs Replace(directOutput, ".xlsm", "-after.xlsx")
            Exit For
        End If
    Next
    path = Replace(directOutput, ".xlsm", "-native.tsv")
    n = DumpNative(StrPtr(path))
    Note "native-records", CStr(n)
    On Error Resume Next
    For Each wb In Application.Workbooks
        If wb.FullName = directTarget Or wb Is directOwner Then wb.Close False
    Next
    On Error GoTo 0
    CompleteDirectCase = ReadAfterOpen()
End Function
Public Function BeginDirectCase() As String
    Dim n As Long
    LoadDirectState
    ReDim trace(0 To 200000)
    used = 0
    loggerReady = True
    '__START_EVENT_SINK__
    If Not eventSink Is Nothing Then Set eventSink.XL = Application
    Stage "tested-direct-com"
    n = MarkNative(1)
    BeginDirectCase = "begin-direct"
End Function
Public Function DirectControlPath() As String
    LoadDirectState
    If Len(directControl) = 0 Then Err.Raise 5, , "Missing direct control state"
    DirectControlPath = directControl
End Function
Private Sub LoadDirectState()
    Dim ws As Worksheet, wb As Workbook
    Set ws = ThisWorkbook.Worksheets(1)
    directTarget = CStr(ws.Range("A2").Value2)
    directOutput = CStr(ws.Range("A3").Value2)
    directControl = CStr(ws.Range("A4").Value2)
    currentCase = CStr(ws.Range("A5").Value2)
    Set observedSheet = ws
    For Each wb In Application.Workbooks
        If wb.FullName = directTarget Then Set directOwner = wb: Exit For
    Next
End Sub
