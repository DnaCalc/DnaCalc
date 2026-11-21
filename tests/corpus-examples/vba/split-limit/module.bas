Attribute VB_Name = "Dc_split_limit_c2dbd5b"
Option Explicit

Public Function DcExample() As Variant
    Dim pieces As Variant
    pieces = Split("x::y::z", "::", 2)
    DcExample = Array(LBound(pieces), UBound(pieces), pieces(1))
End Function
