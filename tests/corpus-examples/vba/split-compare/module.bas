Attribute VB_Name = "Dc_split_compare_0bea903"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array(UBound(Split("AxB", "X", -1, vbTextCompare)), UBound(Split("AxB", "X", -1, vbBinaryCompare)))
End Function
