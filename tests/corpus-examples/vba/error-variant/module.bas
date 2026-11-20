Attribute VB_Name = "Dc_error_variant_b2b9939"
Option Explicit

Public Function DcExample() As Variant
    Dim e As Variant
    e = CVErr(2007)
    DcExample = Array(VarType(e), IsError(e))
End Function
