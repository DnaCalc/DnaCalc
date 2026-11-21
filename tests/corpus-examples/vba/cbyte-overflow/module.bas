Attribute VB_Name = "Dc_cbyte_overflow_d0a8d92"
Option Explicit

Public Function DcExample() As Variant
    Dim result As Byte
    On Error Resume Next
    result = CByte(255.6)
    DcExample = Err.Number
End Function
