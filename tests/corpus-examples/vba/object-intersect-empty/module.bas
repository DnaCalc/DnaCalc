Attribute VB_Name = "Dc_object_intersect_em_926b38f"
Option Explicit

Public Function DcExample() As Variant
    Dim sheet As Worksheet, overlap As Range
    Set sheet = ThisWorkbook.Worksheets(1)
    Set overlap = Application.Intersect(sheet.Range("A1:A2"), sheet.Range("B1:B2"))
    DcExample = (overlap Is Nothing)
End Function
