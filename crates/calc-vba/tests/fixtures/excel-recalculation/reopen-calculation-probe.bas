Private Function CaseIds() As String
    CaseIds = ""
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim p As Variant, wb As Workbook, ws As Worksheet, text As String, line As Variant, r As Range
    p = Split(Split(ident, "/")(0), "~")
    Stage "before-open"
    Application.EnableEvents = True
    If CStr(p(0)) = "auto" Then Application.Calculation = xlCalculationAutomatic Else Application.Calculation = xlCalculationManual
    Note "open-context", "after-controller;mode=" & CStr(Application.Calculation) & ";source=" & CStr(p(1))
    Stage "tested-open"
    Set wb = Application.Workbooks.Open("__OUTPUT__\" & CStr(p(1)), UpdateLinks:=0, ReadOnly:=True)
    Note "mode-after-open", CStr(Application.Calculation)
    For Each ws In wb.Worksheets
        Note "opened-sheet-settings", ws.Name & ";enabled=" & CStr(ws.EnableCalculation)
        Snap ws, "A1:C5,E1,E3:E8,G1:H3"
        For Each r In ws.Range("B2:B5")
            Note "opened-formula-form", ws.Name & "!" & r.Address & ";Formula=" & CStr(r.Formula) & ";Formula2=" & CStr(r.Formula2) & ";R1C1=" & CStr(r.FormulaR1C1) & ";Formula2R1C1=" & CStr(r.Formula2R1C1)
            Note "opened-array-flags", ws.Name & "!" & r.Address & ";hasArray=" & Typed(r.HasArray) & ";hasSpill=" & Typed(r.HasSpill) & ";savedAsArray=" & Typed(r.SavedAsArray)
        Next
    Next
    text = Application.Run("'" & wb.Name & "'!OracleProbe.ReadAfterOpen")
    For Each line In Split(text, vbLf)
        Note "opened-workbook-trace", CStr(line)
    Next
    wb.Close False
    Application.Calculation = xlCalculationManual
End Sub
