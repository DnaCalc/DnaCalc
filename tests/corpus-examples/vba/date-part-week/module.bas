Attribute VB_Name = "Dc_date_part_week_a50dd05"
Option Explicit

Public Function DcExample() As Variant
    Dim d As Date
    d = DateSerial(2021, 1, 1)
    DcExample = Array(DatePart("ww", d, vbMonday, vbFirstJan1), DatePart("ww", d, vbMonday, vbFirstFourDays))
End Function
