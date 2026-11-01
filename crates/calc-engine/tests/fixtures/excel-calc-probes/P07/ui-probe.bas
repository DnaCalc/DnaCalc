Private Declare PtrSafe Function DumpNative Lib "__NATIVE__" (ByVal path As LongPtr) As Long
Private Declare PtrSafe Function ResetNative Lib "__NATIVE__" () As Long
Private Declare PtrSafe Function PendingAsync Lib "__NATIVE__" () As Long
Private Declare PtrSafe Sub Sleep Lib "kernel32" (ByVal milliseconds As Long)
Private Function CaseIds() As String
    CaseIds = "ui"
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim owner As Workbook, path As String, text As String
    path = "__OUTPUT__\P07-ui-owned.xlsm"
    ThisWorkbook.SaveAs output, xlOpenXMLWorkbookMacroEnabled
    ThisWorkbook.SaveCopyAs path
    Set owner = Application.Workbooks.Open(path, UpdateLinks:=0)
    text = Application.Run("'" & owner.Name & "'!OracleProbe.InitializeUI")
    Note "ui-setup", Replace(text, vbLf, "<LF>")
End Sub
Public Function InitializeUI() As String
    currentCase = "ui-seed"
    loggerReady = True
    used = 0
    ReDim trace(0 To 200000)
    '__START_EVENT_SINK__
    Set eventSink.XL = Application
    Set observedSheet = ThisWorkbook.Worksheets(1)
    Application.Calculation = xlCalculationManual
    Application.CalculateBeforeSave = False
    Note "register-xll", CStr(Application.RegisterXLL("__NATIVE__"))
    observedSheet.Range("A1").Value2 = 41
    observedSheet.Range("B1").Value2 = 250
    observedSheet.Range("C1").Formula2 = "=XAfter(AsyncUI(XBefore(A1),B1))"
    observedSheet.Range("D1").Formula2 = "=C1+1"
    Application.Calculate
    WaitNative
    Snap observedSheet, "A1:D1"
    InitializeUI = "seed=" & Typed(observedSheet.Range("C1").Value2)
    ThisWorkbook.Activate
    observedSheet.Range("A1").Select
    ThisWorkbook.Save
End Function
Private Sub WaitNative()
    Dim start As Single
    start = Timer
    Do While PendingAsync() > 0 And Timer - start < 30
        DoEvents
        Sleep 10
    Loop
End Sub
Public Function ArmUI(ByVal ident As String, ByVal value As Double, ByVal delay As Long) As String
    Dim n As Long
    WaitNative
    used = 0
    ReDim trace(0 To 200000)
    n = ResetNative()
    currentCase = ident
    Stage "ui-arm"
    Profile
    observedSheet.Range("A1").Value2 = value
    observedSheet.Range("B1").Value2 = delay
    Snap observedSheet, "A1:D1"
    Note "pending-before-ui", CStr(PendingAsync())
    ThisWorkbook.Activate
    observedSheet.Range("A1").Select
    ArmUI = "armed=" & ident
End Function
Public Function CollectUI(ByVal ident As String) As String
    Dim n As Long, path As String
    WaitNative
    currentCase = ident
    Stage "ui-collect"
    Note "pending-after-ui", CStr(PendingAsync())
    Snap observedSheet, "A1:D1"
    path = "__OUTPUT__\" & ident & "-native.tsv"
    n = DumpNative(StrPtr(path))
    Note "native-records", CStr(n)
    ThisWorkbook.SaveCopyAs "__OUTPUT__\" & ident & ".xlsm"
    CollectUI = ReadAfterOpen()
End Function
