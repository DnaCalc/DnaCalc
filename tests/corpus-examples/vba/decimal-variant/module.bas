Attribute VB_Name = "Dc_decimal_variant_7335c09"
Option Explicit

Public Function DcExample() As Variant
    Dim amount As Variant
    amount = CDec(1) / CDec(8)
    DcExample = Array(VarType(amount), amount)
End Function
