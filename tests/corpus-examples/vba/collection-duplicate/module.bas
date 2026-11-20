Attribute VB_Name = "Dc_collection_duplicat_5eb58d8"
Option Explicit

Public Function DcExample() As Variant
    Dim values As New Collection
    values.Add 1, "item"
    On Error Resume Next
    values.Add 2, "item"
    DcExample = Array(Err.Number, values.Count)
End Function
