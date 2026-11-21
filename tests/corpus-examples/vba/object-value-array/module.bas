Attribute VB_Name = "Dc_object_value_array_221b3d9"
Option Explicit

Public Function DcExample() As Variant
    Dim area As Range, values As Variant
    Set area = ThisWorkbook.Worksheets(1).Range("A1:B2")
    area.Cells(2, 2).Value = 11
    values = area.Value
    DcExample = Array(LBound(values, 1), LBound(values, 2), UBound(values, 2), values(2, 2))
End Function
