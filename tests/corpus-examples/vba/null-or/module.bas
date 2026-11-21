Attribute VB_Name = "Dc_null_or_dc15dc3"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array(True Or Null, IsNull(False Or Null))
End Function
