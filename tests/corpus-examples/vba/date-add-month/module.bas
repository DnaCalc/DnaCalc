Attribute VB_Name = "Dc_date_add_month_eef0514"
Option Explicit

Public Function DcExample() As Variant
    Dim d As Date
    d = DateAdd("m", 1, DateSerial(2024, 1, 31))
    DcExample = Array(Year(d), Month(d), Day(d))
End Function
