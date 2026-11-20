Attribute VB_Name = "Dc_redim_preserve_2a3069b"
Option Explicit

Public Function DcExample() As Variant
    Dim items() As Long
    ReDim items(1 To 2, 4 To 5)
    items(2, 5) = 17
    ReDim Preserve items(1 To 2, 4 To 6)
    DcExample = Array(items(2, 5), items(2, 6), UBound(items, 2))
End Function
