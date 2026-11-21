Attribute VB_Name = "Dc_replace_start_7954d11"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Replace("abc-abc-abc", "abc", "X", 5, 1, vbBinaryCompare)
End Function
