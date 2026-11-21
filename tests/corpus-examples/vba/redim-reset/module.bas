Attribute VB_Name = "Dc_redim_reset_1162642"
Option Explicit

Public Function DcExample() As Variant
    Dim items() As Long
    ReDim items(1 To 2)
    items(1) = 17
    ReDim items(1 To 3)
    DcExample = Array(items(1), UBound(items))
End Function
