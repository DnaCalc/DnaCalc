Attribute VB_Name = "Dc_erase_fixed_ea8a12f"
Option Explicit

Public Function DcExample() As Variant
    Dim items(1 To 2) As String
    items(2) = "x"
    Erase items
    DcExample = Array(UBound(items), items(2))
End Function
