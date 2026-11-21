Attribute VB_Name = "Dc_date_diff_week_69429b8"
Option Explicit

Public Function DcExample() As Variant
    Dim startDate As Date, endDate As Date
    startDate = DateSerial(2024, 1, 6)
    endDate = DateSerial(2024, 1, 7)
    DcExample = Array(DateDiff("d", startDate, endDate), DateDiff("ww", startDate, endDate, vbSunday), DateDiff("ww", startDate, endDate, vbMonday))
End Function
