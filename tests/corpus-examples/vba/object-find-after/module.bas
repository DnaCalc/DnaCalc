Attribute VB_Name = "Dc_object_find_after_168426a"
Option Explicit

Public Function DcExample() As Variant
    Dim area As Range, found As Range
    Set area = ThisWorkbook.Worksheets(1).Range("A1:A3")
    area.Value2 = "needle"
    Set found = area.Find(What:="needle", After:=area.Cells(3), LookIn:=xlValues, LookAt:=xlWhole, SearchOrder:=xlByRows, SearchDirection:=xlNext, MatchCase:=True, SearchFormat:=False)
    DcExample = found.Address(False, False)
End Function
