Attribute VB_Name = "Dc_collection_before_c9d5c78"
Option Explicit

Public Function DcExample() As Variant
    Dim values As New Collection
    values.Add "a", "first"
    values.Add "c", "last"
    values.Add "b", Before:="last"
    DcExample = Array(values(1), values(2), values("last"))
End Function
