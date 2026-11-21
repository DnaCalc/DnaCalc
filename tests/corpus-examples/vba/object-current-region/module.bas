Attribute VB_Name = "Dc_object_current_regi_ca9ad24"
Option Explicit

Public Function DcExample() As Variant
    Dim sheet As Worksheet
    Set sheet = ThisWorkbook.Worksheets(1)
    sheet.Range("A1:B3").Value2 = 1
    sheet.Range("D1:E3").Value2 = 1
    DcExample = sheet.Range("A1").CurrentRegion.Address(False, False)
End Function
