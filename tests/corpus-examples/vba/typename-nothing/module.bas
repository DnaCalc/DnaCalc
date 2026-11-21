Attribute VB_Name = "Dc_typename_nothing_74c6fb5"
Option Explicit

Public Function DcExample() As Variant
    Dim obj As Object
    Dim unset As Variant
    DcExample = Array(TypeName(obj), TypeName(unset), TypeName(Null))
End Function
