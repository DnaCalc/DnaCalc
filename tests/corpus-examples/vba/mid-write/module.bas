Attribute VB_Name = "Dc_mid_write_3606a8f"
Option Explicit

Public Function DcExample() As Variant
    Dim text As String
    text = "abcdef"
    Mid(text, 4, 2) = "WXYZ"
    DcExample = text
End Function
