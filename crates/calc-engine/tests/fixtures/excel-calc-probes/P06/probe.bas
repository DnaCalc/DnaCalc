Private Function CaseIds() As String
    Dim mode As Variant, calc As Variant, method As Variant, events As Variant, t As String
    For Each mode In Array("manual", "except")
        For Each calc In Array("false", "true")
            For Each method In Array("save", "saveas", "copy")
                For Each events In Array("off", "log", "edit")
                    If Len(t) > 0 Then t = t & "|"
                    t = t & CStr(mode) & "-" & CStr(calc) & "-" & CStr(method) & "-" & CStr(events)
                Next
            Next
        Next
    Next
    CaseIds = t & "|manual-false-save-disabled|manual-true-save-disabled|manual-false-save-spill|manual-true-save-spill|except-false-save-table|except-true-save-table|manual-false-save-crossbook|manual-true-save-crossbook|manual-true-save-disabled-inactive"
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Dim ws As Worksheet, other As Worksheet, peer As Workbook, p As Variant, seed As String, tested As String, isCopy As Boolean
    Set ws = observedSheet
    p = Split(Split(ident, "/")(0), "-")
    seed = Replace(output, ".xlsm", "-seed.xlsm")
    tested = Replace(output, ".xlsm", "-tested.xlsm")
    Stage "setup"
    ws.Range("A1").Value2 = 1
    ws.Range("B1").Formula2 = "=A1*2"
    ws.Range("C1").Formula2 = "=B1+1"
    ws.Range("B3").Formula2 = "=PlainProbe(32,A1)"
    ws.Range("C3").Formula2 = "=PlainProbe(33,B3)"
    ws.Range("E1").Formula2 = "=RAND()"
    ws.Range("E3").Formula2 = "=SEQUENCE(A1)"
    If CStr(p(3)) = "crossbook" Then
        Set peer = Application.Workbooks.Add(xlWBATWorksheet)
        peer.Worksheets(1).Range("A1").Value2 = 1
        peer.Worksheets(1).Range("B1").Formula2 = "=A1*2"
        peer.Worksheets(1).Range("C1").Formula2 = "='" & ThisWorkbook.Name & "'!PlainProbe(39,A1)"
    End If
    If CStr(p(3)) = "disabled" Then
        Set other = ThisWorkbook.Worksheets.Add(After:=ws)
        other.Name = "Disabled"
        other.Range("B1").Formula2 = "=S!A1*2"
        other.Range("C1").Formula2 = "=B1+1"
    End If
    If CStr(p(3)) = "table" Then
        ws.Range("Z1").Value2 = 1
        ws.Range("H1").Formula2 = "=A1*Z1"
        ws.Range("G2").Value2 = 2
        ws.Range("G3").Value2 = 3
        ws.Range("G1:H3").Table ColumnInput:=ws.Range("A1")
    End If
    Application.CalculateFull
    Application.Calculate
    If CStr(p(2)) = "save" Then
        ThisWorkbook.SaveAs tested, xlOpenXMLWorkbookMacroEnabled
    Else
        ThisWorkbook.SaveAs seed, xlOpenXMLWorkbookMacroEnabled
    End If
    If Not other Is Nothing Then other.EnableCalculation = False
    If UBound(p) >= 4 Then
        If CStr(p(4)) = "inactive" Then ws.Activate
    End If
    If CStr(p(0)) = "except" Then
        Stage "mode-switch-except"
        Application.Calculation = xlCalculationSemiautomatic
        Snap ws, "A1:C1,B3:C3,E1,E3:E8,G1:H3"
        CountSnapshot
    End If
    Stage "dirty"
    ws.Range("A1").Value2 = 5
    If CStr(p(3)) = "table" Then ws.Range("Z1").Value2 = 10
    If Not peer Is Nothing Then peer.Worksheets(1).Range("A1").Value2 = 5
    Stage "before-save"
    Snap ws, "A1:C1,B3:C3,E1,E3:E8,G1:H3,Z1"
    If Not other Is Nothing Then Snap other, "B1:C1"
    If Not peer Is Nothing Then Snap peer.Worksheets(1), "A1:C1"
    CountSnapshot
    Application.CalculateBeforeSave = CStr(p(1)) = "true"
    Application.EnableEvents = CStr(p(3)) <> "off"
    editOnSave = CStr(p(3)) = "edit"
    Note "tested-settings", "calculateBeforeSave=" & CStr(Application.CalculateBeforeSave) & ";events=" & CStr(Application.EnableEvents) & ";mode=" & CStr(Application.Calculation) & ";activeWorkbook=" & ActiveWorkbook.Name & ";activeSheet=" & ActiveSheet.Name & ";S-enabled=" & CStr(ws.EnableCalculation)
    If Not other Is Nothing Then Note "disabled-setting", "other-enabled=" & CStr(other.EnableCalculation)
    Stage "tested-save"
    Select Case CStr(p(2))
    Case "save": ThisWorkbook.Save
    Case "saveas": ThisWorkbook.SaveAs tested, xlOpenXMLWorkbookMacroEnabled
    Case "copy": ThisWorkbook.SaveCopyAs tested
    End Select
    Note "save-returned", tested
    Stage "after-save"
    Snap ws, "A1:C1,B3:C3,E1,E3:E8,G1:H3,Z1"
    If Not other Is Nothing Then Snap other, "B1:C1"
    If Not peer Is Nothing Then Snap peer.Worksheets(1), "A1:C1"
    CountSnapshot
    Application.CalculateBeforeSave = False
    Application.Calculation = xlCalculationManual
    Application.EnableEvents = True
    If Not peer Is Nothing Then
        peer.SaveAs Replace(output, ".xlsm", "-other.xlsx"), xlOpenXMLWorkbook
        peer.Close False
    End If
End Sub
