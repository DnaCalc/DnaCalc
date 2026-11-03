Private Sub Workbook_SheetCalculate(ByVal Sh As Object)
    P22Probe.Observe "Workbook_SheetCalculate", Sh.Name
End Sub
Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)
    P22Probe.BeforeSave "Workbook", SaveAsUI, Cancel
End Sub