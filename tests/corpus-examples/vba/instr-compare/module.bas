Attribute VB_Name = "Dc_instr_compare_fb280f9"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array(InStr(1, "abCD", "cd", vbBinaryCompare), InStr(1, "abCD", "cd", vbTextCompare))
End Function
