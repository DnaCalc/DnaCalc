Private Sub Worksheet_Change(ByVal Target As Range)
    ThisWorkbook.Worksheets({{event_sheet}}).Range({{event_cell}}).Value2 = "Change"
End Sub
