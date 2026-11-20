Attribute VB_Name = "Dc_erase_dynamic_9a4c5d3"
Option Explicit

Public Function DcExample() As Variant
    Dim items() As Long, bound As Long
    ReDim items(1 To 2)
    Erase items
    On Error Resume Next
    bound = UBound(items)
    DcExample = Err.Number
End Function
