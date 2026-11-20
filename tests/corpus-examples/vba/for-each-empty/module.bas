Attribute VB_Name = "Dc_for_each_empty_14d9b15"
Option Explicit

Public Function DcExample() As Variant
    Dim values As New Collection, item As Variant
    Dim count As Long
    For Each item In values
        count = count + 1
    Next item
    DcExample = count
End Function
