Attribute VB_Name = "Dc_switch_eager_9fcfbe8"
Option Explicit

Public Function DcExample() As Variant
    Dim value As Variant
    On Error Resume Next
    value = Switch(True, "chosen", 1 / 0 = 0, "other")
    DcExample = Err.Number
End Function
