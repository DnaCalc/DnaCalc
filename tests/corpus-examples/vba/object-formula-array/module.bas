Attribute VB_Name = "Dc_object_formula_arra_64b1672"
Option Explicit

Public Function DcExample() As Variant
    Dim area As Range, items As Variant
    Set area = ThisWorkbook.Worksheets(1).Range("A1:A2")
    area.Cells(1).Value2 = 4
    area.Cells(2).Formula = "=A1+1"
    items = area.Formula
    DcExample = Array(items(1, 1), items(2, 1))
End Function
