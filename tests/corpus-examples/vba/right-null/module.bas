Attribute VB_Name = "Dc_right_null_a4f7522"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array(IsNull(Right(Null, 1)), Right("abc", 2))
End Function
