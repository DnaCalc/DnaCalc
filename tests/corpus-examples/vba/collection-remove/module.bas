Attribute VB_Name = "Dc_collection_remove_d6431e0"
Option Explicit

Public Function DcExample() As Variant
    Dim values As New Collection
    values.Add "a", "first"
    values.Add "b", "second"
    values.Remove "first"
    DcExample = Array(values.Count, values.Item(1))
End Function
