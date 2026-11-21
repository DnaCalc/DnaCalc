Attribute VB_Name = "Dc_len_lenb_e400d8c"
Option Explicit

Public Function DcExample() As Variant
    Dim text As String
    text = "A" & ChrW(&H20AC)
    DcExample = Array(Len(text), LenB(text))
End Function
