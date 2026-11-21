Attribute VB_Name = "Dc_instr_empty_a366318"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array(InStr(3, "abcde", ""), InStr(1, "", "a"))
End Function
