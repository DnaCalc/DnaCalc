Attribute VB_Name = "Dc_filter_empty_5929b3f"
Option Explicit

Public Function DcExample() As Variant
    Dim items As Variant, bound As Long
    items = Array("ab", "BC", "zz")
    On Error Resume Next
    bound = UBound(Filter(items, "qq"))
    DcExample = Array(Join(Filter(items, "b", False, vbTextCompare), ","), bound, Err.Number)
End Function
