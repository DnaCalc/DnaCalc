Attribute VB_Name = "Dc_format_sections_ac61dd4"
Option Explicit

Public Function DcExample() As Variant
    Dim pattern As String
    pattern = "0;[0];""zero"";""missing"""
    DcExample = Array(Format(2, pattern), Format(0, pattern), Format(Null, pattern))
End Function
