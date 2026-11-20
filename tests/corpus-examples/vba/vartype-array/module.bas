Attribute VB_Name = "Dc_vartype_array_d1b1257"
Option Explicit

Public Function DcExample() As Variant
    Dim items(0 To 1) As Integer
    DcExample = Array(VarType(items), VarType(Array(1, 2)))
End Function
