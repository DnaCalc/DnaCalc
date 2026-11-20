Attribute VB_Name = "Dc_object_end_blank_6855bfb"
Option Explicit

Public Function DcExample() As Variant
    Dim sheet As Worksheet
    Set sheet = ThisWorkbook.Worksheets(1)
    sheet.Range("A1:A3").Value2 = 1
    sheet.Range("A5").Value2 = 1
    DcExample = sheet.Range("A1").End(xlDown).Address(False, False)
End Function
