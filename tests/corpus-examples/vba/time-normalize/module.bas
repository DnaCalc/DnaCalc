Attribute VB_Name = "Dc_time_normalize_f6e8dc6"
Option Explicit

Public Function DcExample() As Variant
    Dim t As Date
    t = TimeSerial(25, -30, 0)
    DcExample = Array(Hour(t), Minute(t))
End Function
