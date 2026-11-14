Attribute VB_Name = "BuildReport"
Option Explicit
' dc-piw0.4: the macro-built report. One macro, run in Excel (the answer) and in DNA Calc (compared).
' Builds, on the active sheet of the active workbook: the data of report-data.tsv (values and a formula
' column), a table "Sales" over it, a column chart over the data with a title, and four drawing objects:
' two auto shapes with text joined by a straight connector, a text box and a line.
' Only members registered in the object-model descriptor are used. After it, run DumpReport.Dump.

' The data grid: tab-separated rows; a cell that starts with = is a formula, a number is a number.
Private Function DataLines() As Variant
    DataLines = Array( _
        "Region" & vbTab & "Q1" & vbTab & "Q2" & vbTab & "Total", _
        "North" & vbTab & "10" & vbTab & "20" & vbTab & "=B2+C2", _
        "South" & vbTab & "30" & vbTab & "40" & vbTab & "=B3+C3", _
        "East" & vbTab & "50" & vbTab & "60" & vbTab & "=B4+C4", _
        "West" & vbTab & "70" & vbTab & "80" & vbTab & "=B5+C5", _
        "Central" & vbTab & "90" & vbTab & "100" & vbTab & "=B6+C6")
End Function

Public Sub Build()
    Dim ws As Worksheet: Set ws = ActiveSheet
    Dim lines As Variant: lines = DataLines()
    Dim r As Long, c As Long, parts() As String
    For r = 0 To UBound(lines)
        parts = Split(lines(r), vbTab)
        For c = 0 To UBound(parts)
            If Left(parts(c), 1) = "=" Then
                ws.Range(Chr(65 + c) & CStr(r + 1)).Formula = parts(c)
            ElseIf IsNumeric(parts(c)) Then
                ws.Range(Chr(65 + c) & CStr(r + 1)).Value2 = CDbl(parts(c))
            Else
                ws.Range(Chr(65 + c) & CStr(r + 1)).Value2 = parts(c)
            End If
        Next
    Next

    ' The table.
    Dim lo As ListObject
    Set lo = ws.ListObjects.Add(xlSrcRange, ws.Range("A1:D6"), , xlYes)
    lo.Name = "Sales"
    lo.TableStyle = "TableStyleMedium4"

    ' The chart over the region and the two quarters.
    Dim co As ChartObject
    Set co = ws.ChartObjects.Add(300, 20, 360, 220)
    co.Name = "SalesChart"
    co.Chart.SetSourceData ws.Range("A1:C6")
    co.Chart.HasTitle = True
    co.Chart.ChartTitle.Text = "Sales by region"
    co.Chart.HasLegend = True

    ' The drawing objects.
    Dim startBox As Shape, endBox As Shape, link As Shape, note As Shape, rule As Shape
    Set startBox = ws.Shapes.AddShape(msoShapeRectangle, 20, 200, 100, 50)
    startBox.Name = "Start"
    startBox.TextFrame2.TextRange.Text = "Start"
    startBox.Fill.ForeColor.RGB = RGB(255, 128, 0)
    startBox.Line.Weight = 2
    Set endBox = ws.Shapes.AddShape(msoShapeOval, 200, 300, 100, 50)
    endBox.Name = "Finish"
    endBox.TextFrame2.TextRange.Text = "Finish"
    Set link = ws.Shapes.AddConnector(msoConnectorStraight, 0, 0, 10, 10)
    link.Name = "Link"
    link.ConnectorFormat.BeginConnect startBox, 3
    link.ConnectorFormat.EndConnect endBox, 1
    Set note = ws.Shapes.AddTextbox(msoTextOrientationHorizontal, 160, 240, 120, 30)
    note.Name = "Note"
    note.TextFrame2.TextRange.Text = "Quarterly sales"
    Set rule = ws.Shapes.AddLine(20, 380, 300, 380)
    rule.Name = "Rule"
End Sub
