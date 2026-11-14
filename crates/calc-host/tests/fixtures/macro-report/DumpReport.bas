Attribute VB_Name = "DumpReport"
Option Explicit
' dc-piw0.4: the listing macro. Run after BuildReport.Build, in Excel and in DNA Calc; the two listings are
' diffed line by line. Writes tab-separated lines (one record per line) to the sheet "Listing" of a NEW
' workbook and to a text file (ListingPath). The Rust test dc_piw0_4__report_listing in
' crates/calc-host/tests/automation.rs prints the same records through the object-model handlers.
'
' Records (tab-separated; numbers as CStr writes them; lists joined with |):
'   CELL    address  Value2                                 each cell of the used range, row-major
'   FORMULA address  Formula                                formula cells only (DNA Calc lists it once Range.Formula answers)
'   TABLE   name  range  style  header names | ...  rows   each ListObject
'   CHART   name  ChartType  left top width height  HasTitle  title  HasLegend  series count
'   SERIES  chart name  index  name  formula  plot order    each series
'   SHAPE   name  id  type  left top width height  text  begin shape  begin site  end shape  end site
'           every Shapes item (a chart is a shape of type 3); text for auto shapes and text boxes;
'           begin/end for connectors (empty otherwise)

Public Const ListingPath As String = "C:\Work\DnaCalc\codex-runs\eng3-excel\report-listing.txt"
Private out As String

Private Sub L(ParamArray a() As Variant)
    Dim i As Long, s As String
    For i = LBound(a) To UBound(a)
        If i > LBound(a) Then s = s & vbTab
        s = s & CStr(a(i))
    Next
    out = out & s & vbLf
End Sub

Private Function Safe(v As Variant) As String
    Safe = CStr(v)
End Function

Public Sub Dump()
    Dim ws As Worksheet: Set ws = ActiveSheet
    out = ""
    Dim r As Long, c As Long, cell As Range
    ' Cells of the data region A1:D6 and below the table (the used range is not a registered member).
    For r = 1 To 6
        For c = 1 To 4
            Set cell = ws.Range(Chr(64 + c) & CStr(r))
            L "CELL", cell.Address(False, False), cell.Value2
            If Left(CStr(cell.Formula), 1) = "=" Then L "FORMULA", cell.Address(False, False), cell.Formula
        Next
    Next
    Dim lo As ListObject, i As Long, names As String, t As Long
    For t = 1 To ws.ListObjects.Count
        Set lo = ws.ListObjects(t)
        names = ""
        For i = 1 To lo.ListColumns.Count
            names = names & IIf(i > 1, "|", "") & lo.ListColumns(i).Name
        Next
        L "TABLE", lo.Name, lo.Range.Address, lo.TableStyle, names, lo.ListRows.Count
    Next
    Dim co As ChartObject, s As Long, ch As Chart
    For t = 1 To ws.ChartObjects.Count
        Set co = ws.ChartObjects(t)
        Set ch = co.Chart
        L "CHART", co.Name, ch.ChartType, co.Left, co.Top, co.Width, co.Height, ch.HasTitle, _
            IIf(ch.HasTitle, ch.ChartTitle.Text, ""), ch.HasLegend, ch.SeriesCollection.Count
        For s = 1 To ch.SeriesCollection.Count
            L "SERIES", co.Name, s, ch.SeriesCollection(s).Name, ch.SeriesCollection(s).Formula, ch.SeriesCollection(s).PlotOrder
        Next
    Next
    Dim sh As Shape, txt As String, b As String, bs As String, e As String, es As String
    For i = 1 To ws.Shapes.Count
        Set sh = ws.Shapes(i)
        txt = "": b = "": bs = "": e = "": es = ""
        If sh.Type = msoAutoShape Or sh.Type = msoTextBox Then
            If Not sh.Connector Then txt = sh.TextFrame2.TextRange.Text
        End If
        If sh.Connector Then
            If sh.ConnectorFormat.BeginConnected Then b = sh.ConnectorFormat.BeginConnectedShape.Name: bs = sh.ConnectorFormat.BeginConnectionSite
            If sh.ConnectorFormat.EndConnected Then e = sh.ConnectorFormat.EndConnectedShape.Name: es = sh.ConnectorFormat.EndConnectionSite
        End If
        L "SHAPE", sh.Name, sh.ID, sh.Type, sh.Left, sh.Top, sh.Width, sh.Height, txt, b, bs, e, es
    Next
    ' To the sheet "Listing" of a new workbook, and to the text file.
    Dim lines() As String, parts() As String, j As Long, lb As Workbook
    Set lb = Workbooks.Add
    lines = Split(out, vbLf)
    For i = 0 To UBound(lines)
        parts = Split(lines(i), vbTab)
        For j = 0 To UBound(parts)
            lb.Worksheets(1).Cells(i + 1, j + 1).NumberFormat = "@"
            lb.Worksheets(1).Cells(i + 1, j + 1).Value = parts(j)
        Next
    Next
    lb.Worksheets(1).Name = "Listing"
    Dim f As Integer: f = FreeFile
    Open ListingPath For Output As #f
    Print #f, out;
    Close #f
End Sub
