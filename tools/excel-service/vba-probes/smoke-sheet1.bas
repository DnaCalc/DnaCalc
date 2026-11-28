Option Explicit
Private busy As Boolean
Private Sub Worksheet_Change(ByVal Target As Range)
    If busy Then Exit Sub
    busy = True
    ThisWorkbook.Worksheets({{event_sheet}}).Range({{event_cell}}).Value2 = "Change"
    busy = False
End Sub
