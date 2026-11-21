Attribute VB_Name = "Dc_null_concatenation_6dca290"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array("x" & Null, IsNull(Null & Null))
End Function
