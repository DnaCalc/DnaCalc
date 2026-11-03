Public WithEvents App As Excel.Application
Private Sub App_SheetChange(ByVal Sh As Object, ByVal Target As Range)
    If Sh.Parent Is ThisWorkbook Then P22Probe.Observe "Application_SheetChange", Sh.Name & "!" & Target.Address(False, False)
End Sub
Private Sub App_SheetCalculate(ByVal Sh As Object)
    If Sh.Parent Is ThisWorkbook Then P22Probe.Observe "Application_SheetCalculate", Sh.Name
End Sub
Private Sub App_WorkbookBeforeSave(ByVal Wb As Workbook, ByVal SaveAsUI As Boolean, Cancel As Boolean)
    If Wb Is ThisWorkbook Then P22Probe.BeforeSave "Application", SaveAsUI, Cancel
End Sub