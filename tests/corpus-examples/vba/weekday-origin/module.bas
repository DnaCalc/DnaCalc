Attribute VB_Name = "Dc_weekday_origin_74a08e0"
Option Explicit

Public Function DcExample() As Variant
    Dim d As Date
    d = DateSerial(2024, 1, 1)
    DcExample = Array(Weekday(d, vbSunday), Weekday(d, vbMonday))
End Function
