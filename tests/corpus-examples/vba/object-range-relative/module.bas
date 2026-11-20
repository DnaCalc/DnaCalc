Attribute VB_Name = "Dc_object_range_relati_f3602de"
Option Explicit

Public Function DcExample() As Variant
    Dim area As Range
    Set area = ThisWorkbook.Worksheets(1).Range("B2:C3")
    DcExample = area.Range("A1:B1").Address(False, False)
End Function
