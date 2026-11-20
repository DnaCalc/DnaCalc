Attribute VB_Name = "Dc_object_columns_firs_463a549"
Option Explicit

Public Function DcExample() As Variant
    Dim sheet As Worksheet, area As Range
    Set sheet = ThisWorkbook.Worksheets(1)
    Set area = Union(sheet.Range("A1:B2"), sheet.Range("D1:F2"))
    DcExample = Array(area.Areas.Count, area.Columns.Count)
End Function
