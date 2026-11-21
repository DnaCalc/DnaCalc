Attribute VB_Name = "Dc_object_resize_omit_5b71739"
Option Explicit

Public Function DcExample() As Variant
    Dim area As Range
    Set area = ThisWorkbook.Worksheets(1).Range("B2:C4").Resize(ColumnSize:=1)
    DcExample = area.Address(False, False)
End Function
