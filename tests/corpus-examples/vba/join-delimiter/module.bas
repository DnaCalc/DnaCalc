Attribute VB_Name = "Dc_join_delimiter_331f6f4"
Option Explicit

Public Function DcExample() As Variant
    Dim items As Variant
    items = Array("a", "", "c")
    DcExample = Array(Join(items, ""), Join(items, "::"))
End Function
