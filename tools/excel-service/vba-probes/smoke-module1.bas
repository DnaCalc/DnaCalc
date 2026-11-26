Option Explicit
Public Function Probe(ByVal x As Variant) As Variant
    ThisWorkbook.Worksheets({{target_sheet}}).Range({{target_cell}}).Value2 = x
    Probe = x * 2
End Function
Public Function ErrProbe() As Variant
    On Error GoTo 100
    Err.Raise 5, "Module1", {{message}}
    Exit Function
100:
    ErrProbe = Array("Touchstone.Vba.Err", CLng(Err.Number), Err.Description)
End Function
