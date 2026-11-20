Attribute VB_Name = "Dc_bounds_rank_f66eb20"
Option Explicit

Public Function DcExample() As Variant
    Dim items(-2 To 3, 4 To 7) As Long
    DcExample = Array(LBound(items, 1), UBound(items, 1), LBound(items, 2), UBound(items, 2))
End Function
