Attribute VB_Name = "Dc_for_negative_step_e0d7c01"
Option Explicit

Public Function DcExample() As Variant
    Dim index As Long, trace As String
    For index = 5 To 1 Step -2
        trace = trace & CStr(index)
    Next index
    DcExample = Array(trace, index)
End Function
