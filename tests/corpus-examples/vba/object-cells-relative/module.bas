Attribute VB_Name = "Dc_object_cells_relati_4716c3a"
Option Explicit

Public Function DcExample() As Variant
    Dim area As Range
    Set area = ThisWorkbook.Worksheets(1).Range("B2:C3")
    DcExample = area.Cells(4, 3).Address(False, False)
End Function
