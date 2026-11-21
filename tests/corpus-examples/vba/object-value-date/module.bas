Attribute VB_Name = "Dc_object_value_date_cac1b5f"
Option Explicit

Public Function DcExample() As Variant
    Dim target As Range
    Set target = ThisWorkbook.Worksheets(1).Range("A1")
    target.NumberFormat = "yyyy-mm-dd"
    target.Value2 = 45293
    DcExample = Array(VarType(target.Value), VarType(target.Value2))
End Function
