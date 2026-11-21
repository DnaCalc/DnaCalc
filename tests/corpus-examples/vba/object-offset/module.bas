Attribute VB_Name = "Dc_object_offset_cdeae49"
Option Explicit

Public Function DcExample() As Variant
    Dim target As Range
    Set target = ThisWorkbook.Worksheets(1).Range("C3:D4").Offset(1, -1)
    DcExample = target.Address(False, False)
End Function
