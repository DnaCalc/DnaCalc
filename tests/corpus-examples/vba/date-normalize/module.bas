Attribute VB_Name = "Dc_date_normalize_64accd1"
Option Explicit

Public Function DcExample() As Variant
    Dim d As Date
    d = DateSerial(2024, 13, 0)
    DcExample = Array(Year(d), Month(d), Day(d))
End Function
