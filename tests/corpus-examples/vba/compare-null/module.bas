Attribute VB_Name = "Dc_compare_null_bcfafef"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array(StrComp("a", "A", vbTextCompare), StrComp("a", "A", vbBinaryCompare), IsNull(StrComp(Null, "A")))
End Function
