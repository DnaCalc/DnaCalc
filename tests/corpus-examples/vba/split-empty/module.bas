Attribute VB_Name = "Dc_split_empty_7c7f7fd"
Option Explicit

Public Function DcExample() As Variant
    Dim a As Variant, b As Variant
    Dim emptyBound As Long
    a = Split("")
    b = Split("one", "")
    On Error Resume Next
    emptyBound = UBound(a)
    DcExample = Array(emptyBound, Err.Number, UBound(b), b(0))
End Function
