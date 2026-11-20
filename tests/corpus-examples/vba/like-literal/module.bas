Attribute VB_Name = "Dc_like_literal_2897229"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array("a*b" Like "a[*]b", "axb" Like "a[*]b", "[x" Like "[[]?")
End Function
