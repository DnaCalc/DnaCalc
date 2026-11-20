Attribute VB_Name = "Dc_object_special_cons_2cac447"
Option Explicit

Public Function DcExample() As Variant
    Dim area As Range
    Set area = ThisWorkbook.Worksheets(1).Range("A1:A4")
    area.Cells(1).Value2 = 7
    area.Cells(2).Value2 = "text"
    area.Cells(3).Formula = "=3+2"
    DcExample = area.SpecialCells(xlCellTypeConstants, xlNumbers).Address(False, False)
End Function
