Attribute VB_Name = "Dc_for_evaluated_once_1f9c9f4"
Option Explicit

Public Function DcExample() As Variant
    Dim i As Long, limit As Long, count As Long
    limit = 3
    For i = 1 To limit
        count = count + 1
        limit = 1
    Next i
    DcExample = count
End Function
