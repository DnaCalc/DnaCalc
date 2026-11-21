Attribute VB_Name = "Dc_err_clear_e3d01b2"
Option Explicit

Public Function DcExample() As Variant
    Dim before As Long
    Dim unused As Variant
    On Error Resume Next
    unused = CInt("text")
    before = Err.Number
    Err.Clear
    DcExample = Array(before, Err.Number)
End Function
