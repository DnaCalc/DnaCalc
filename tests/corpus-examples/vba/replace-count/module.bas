Attribute VB_Name = "Dc_replace_count_72b121c"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array(Replace("aaAAaa", "aa", "X", 1, 2, vbTextCompare), Replace("aaAAaa", "aa", "X", 1, 2, vbBinaryCompare))
End Function
