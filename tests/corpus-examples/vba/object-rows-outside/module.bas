Attribute VB_Name = "Dc_object_rows_outside_7ddbcdc"
Option Explicit

Public Function DcExample() As Variant
    Dim area As Range
    Set area = ThisWorkbook.Worksheets(1).Range("B2:C3")
    DcExample = area.Rows(5).Address(False, False)
End Function
