Attribute VB_Name = "Dc_with_object_ec1f0a7"
Option Explicit

Public Function DcExample() As Variant
    Dim a As New Collection, b As New Collection
    Dim selected As Collection
    Set selected = a
    With selected
        Set selected = b
        .Add "original"
    End With
    DcExample = Array(a.Count, b.Count)
End Function
