Attribute VB_Name = "Dc_object_address_r1c1_f432951"
Option Explicit

Public Function DcExample() As Variant
    Dim sheet As Worksheet
    Set sheet = ThisWorkbook.Worksheets(1)
    DcExample = sheet.Range("E7").Address(False, False, xlR1C1, False, sheet.Range("C4"))
End Function
