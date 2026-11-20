Attribute VB_Name = "Dc_iif_eager_24c84b8"
Option Explicit

Public Function DcExample() As Variant
    Dim value As Variant
    On Error Resume Next
    value = IIf(True, 1, 1 / 0)
    DcExample = Err.Number
End Function
