Attribute VB_Name = "Dc_val_type_suffix_a78c346"
Option Explicit

Public Function DcExample() As Variant
    Dim result As Double
    On Error Resume Next
    result = Val("12.5%")
    DcExample = Err.Number
End Function
