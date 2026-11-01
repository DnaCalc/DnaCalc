Private Function CaseIds() As String
    CaseIds = "insert-above|delete-above|cut-move|delete-held|multi-area-insert|switch-active|close-owner|close-reopen-owner|module-reload"
End Function
Private Sub ProviderState(provider As Workbook)
    Dim value As String, line As Variant
    value = Application.Run("'" & provider.Name & "'!Provider.StateSummary")
    Note "provider-state", value
    value = Application.Run("'" & provider.Name & "'!Provider.HeldProperties")
    Note "held-properties", value
    value = Application.Run("'" & provider.Name & "'!Provider.ReadLog")
    For Each line In Split(value, vbLf)
        Note "provider-trace", CStr(line)
    Next
End Sub
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim provider As Workbook, owner As Workbook, other As Workbook, ws As Worksheet, control As Worksheet
    Dim component As Object, source As String, ownerPath As String, providerPath As String, p As String, f As Integer, text As String
    Set control = observedSheet
    p = Split(ident, "/")(0)
    Stage "setup-provider"
    f = FreeFile
    Open "__OUTPUT__\provider.bas" For Input As #f
    source = Input$(LOF(f), f)
    Close #f
    Set provider = Application.Workbooks.Add(xlWBATWorksheet)
    Set component = provider.VBProject.VBComponents.Add(1)
    component.Name = "Provider"
    component.CodeModule.AddFromString source
    providerPath = Replace(output, ".xlsm", "-provider.xlam")
    provider.SaveAs providerPath, xlOpenXMLAddIn
    provider.IsAddin = True
    Set owner = Application.Workbooks.Add(xlWBATWorksheet)
    Set ws = owner.Worksheets(1)
    ws.Name = "Owner"
    ws.Range("A5").Value2 = 42
    ws.Range("A6").Value2 = 99
    ws.Range("A7").Value2 = 72
    If p = "multi-area-insert" Then
        ws.Range("B1").Formula2 = "='" & provider.Name & "'!Hold((A5,A7))"
    Else
        ws.Range("B1").Formula2 = "='" & provider.Name & "'!Hold(A5)"
    End If
    control.Range("A1").Value2 = 5
    control.Range("C1").Formula2 = "='" & provider.Name & "'!ReadHeld()"
    control.Range("F1").Formula2 = "='" & provider.Name & "'!F(A1)"
    control.Range("G1").Formula2 = "='" & provider.Name & "'!Stateful()"
    ws.Range("B1").Calculate
    control.Range("C1,F1,G1").Calculate
    Application.Calculate
    ownerPath = Replace(output, ".xlsm", "-owner.xlsx")
    owner.SaveAs ownerPath, xlOpenXMLWorkbook
    Stage "accepted-seed"
    Snap control, "A1,C1,F1:G1"
    ProviderState provider
    If IsError(control.Range("C1").Value2) Or IsError(control.Range("F1").Value2) Then Err.Raise 5, , "Invalid provider seed: reader or F returned an error"
    If control.Range("C1").Value2 <> 42 Or control.Range("F1").Value2 <> 10 Then Err.Raise 5, , "Invalid provider seed: expected held 42 and F(5)=10"
    Stage "tested-operation"
    Select Case p
    Case "insert-above", "multi-area-insert": ws.Rows(3).Insert
    Case "delete-above": ws.Rows(3).Delete
    Case "cut-move": ws.Range("A5").Cut Destination:=ws.Range("D9")
    Case "delete-held": ws.Rows(5).Delete
    Case "switch-active"
        Set other = Application.Workbooks.Add(xlWBATWorksheet)
        other.Activate
    Case "close-owner", "close-reopen-owner"
        owner.Close False
        If p = "close-reopen-owner" Then Set owner = Application.Workbooks.Open(ownerPath, UpdateLinks:=0)
    Case "module-reload"
        source = component.CodeModule.Lines(1, component.CodeModule.CountOfLines)
        source = Replace(source, "ByVal x As Double", "ByVal x As String")
        source = Replace(source, "F = x * 2", "F = ""text:"" & x")
        component.CodeModule.DeleteLines 1, component.CodeModule.CountOfLines
        component.CodeModule.AddFromString source
        Note "module-source", Replace(Replace(source, vbCrLf, "<CRLF>"), vbLf, "<LF>")
    End Select
    Stage "after-operation-before-f9"
    ProviderState provider
    Snap control, "C1,F1:G1"
    Stage "f9"
    Application.Calculate
    ProviderState provider
    Snap control, "C1,F1:G1"
    Stage "force-reader-and-callers"
    control.Range("C1,F1,G1").Calculate
    ProviderState provider
    Snap control, "C1,F1:G1"
    provider.SaveCopyAs Replace(output, ".xlsm", "-provider-after.xlam")
    If p <> "close-owner" Then owner.SaveCopyAs Replace(output, ".xlsm", "-owner-after.xlsx")
    On Error Resume Next
    If Not other Is Nothing Then other.Close False
    owner.Close False
    provider.Close False
    On Error GoTo 0
End Sub
