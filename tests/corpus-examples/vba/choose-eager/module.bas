Attribute VB_Name = "Dc_choose_eager_f23d4e3"
Option Explicit

Public Function DcExample() As Variant
    Dim value As Variant
    On Error Resume Next
    value = Choose(1, "selected", 1 / 0)
    DcExample = Err.Number
End Function
