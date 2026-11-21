Attribute VB_Name = "Dc_empty_context_70f0c23"
Option Explicit

Public Function DcExample() As Variant
    Dim value As Variant
    DcExample = Array(IsEmpty(value), value + 2, "[" & value & "]")
End Function
