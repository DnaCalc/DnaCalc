Private Function CaseIds() As String
    CaseIds = ""
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim p As Variant, wb As Workbook, text As String, line As Variant
    p = Split(Split(ident, "/")(0), "~")
    Stage "before-open"
    Application.EnableEvents = True
    If CStr(p(0)) = "auto" Then Application.Calculation = xlCalculationAutomatic Else Application.Calculation = xlCalculationManual
    Note "open-context", "after-controller;mode=" & CStr(Application.Calculation) & ";source=" & CStr(p(1))
    Stage "tested-open"
    Set wb = Application.Workbooks.Open("__OUTPUT__\" & CStr(p(1)), UpdateLinks:=0, ReadOnly:=True)
    Note "mode-after-open", CStr(Application.Calculation)
    Snap wb.Worksheets(1), "A1:C5,E1,E3:E8,G1:H3"
    text = Application.Run("'" & wb.Name & "'!OracleProbe.ReadAfterOpen")
    For Each line In Split(text, vbLf)
        Note "opened-workbook-trace", CStr(line)
    Next
    wb.Close False
    Application.Calculation = xlCalculationManual
End Sub
