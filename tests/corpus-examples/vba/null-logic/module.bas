Attribute VB_Name = "Dc_null_logic_2e87555"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array(False And Null, IsNull(True And Null))
End Function
