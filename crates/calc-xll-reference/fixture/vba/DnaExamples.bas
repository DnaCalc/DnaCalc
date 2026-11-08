Attribute VB_Name = "DnaExamples"
Option Explicit

' DNA Calc's example VBA add-in (dc-0f1k.4): worksheet functions that OneCalc and Excel
' should answer alike. Built into DnaExamples.xlam and DnaExamples.xlsm by build-job.ps1.

Public DnaOpened As Long
Public DnaInstalled As Boolean

Public Function DnaAdd(ByVal a As Double, ByVal b As Double) As Double
    DnaAdd = a + b
End Function

Public Function DnaGreet(ByVal name As String) As String
    DnaGreet = "Hello, " & name
End Function

Public Function DnaJoin(ByVal a As String, Optional ByVal sep As String = "-", Optional ByVal b As Variant) As String
    If IsMissing(b) Then
        DnaJoin = a & sep & "(none)"
    Else
        DnaJoin = a & sep & CStr(b)
    End If
End Function

Public Function DnaSum(ParamArray values() As Variant) As Double
    Dim v As Variant, total As Double
    For Each v In values
        If Not IsMissing(v) Then total = total + CDbl(v)
    Next
    DnaSum = total
End Function

Public Function DnaArgs(ParamArray values() As Variant) As String
    Dim i As Long, text As String
    text = CStr(UBound(values) - LBound(values) + 1)
    For i = LBound(values) To UBound(values)
        If IsMissing(values(i)) Then
            text = text & ":missing"
        Else
            text = text & ":" & TypeName(values(i))
        End If
    Next
    DnaArgs = text
End Function

Public Function DnaCounted(ByVal a As Long, ByVal b As Long) As Long
    Dim counter As New DnaCounter
    counter.Add a
    counter.Add b
    DnaCounted = counter.Total
End Function

Public Function DnaCounterName() As String
    Dim counter As DnaCounter
    Set counter = New DnaCounter
    DnaCounterName = TypeName(counter)
End Function

Public Function DnaDistinct(ParamArray keys() As Variant) As Long
    Dim seen As New Scripting.Dictionary
    Dim k As Variant
    seen.CompareMode = TextCompare
    For Each k In keys
        If Not seen.Exists(k) Then seen.Add k, True
    Next
    DnaDistinct = seen.Count
End Function

Public Function DnaKeys(ParamArray keys() As Variant) As String
    Dim seen As Scripting.Dictionary
    Dim k As Variant
    Set seen = New Scripting.Dictionary
    For Each k In keys
        seen(k) = seen(k) + 1
    Next
    DnaKeys = Join(seen.keys, ",") & "|" & CStr(seen.Item(keys(0)))
End Function

Public Function DnaDictType() As String
    Dim d As New Scripting.Dictionary
    DnaDictType = TypeName(d)
End Function

Public Function DnaCells(ByVal r As Range) As Long
    DnaCells = r.Count
End Function

Public Function DnaWhere(ByVal r As Range) As String
    DnaWhere = r.Address
End Function

Public Function DnaWsSum(ByVal a As Double, ByVal b As Double) As Double
    DnaWsSum = Application.WorksheetFunction.Sum(a, b)
End Function

Public Function DnaNA() As Variant
    DnaNA = CVErr(2042)
End Function

Public Function DnaFail() As Variant
    Err.Raise 5
End Function

Public Function DnaOpenCount() As Long
    DnaOpenCount = DnaOpened
End Function

Public Function DnaWasInstalled() As Boolean
    DnaWasInstalled = DnaInstalled
End Function

Public Function DnaTryWrite() As Long
    On Error Resume Next
    ActiveSheet.Range("Z99").Value = 1
    DnaTryWrite = Err.Number
End Function

Public Function DnaWrite() As Long
    ActiveSheet.Range("Z99").Value = 1
    DnaWrite = 1
End Function

Public Function DnaTryWriteArg(ByVal r As Range) As Long
    On Error Resume Next
    r.Value = 1
    DnaTryWriteArg = Err.Number
End Function

Public Function DnaWriteArg(ByVal r As Range) As Long
    r.Value = 1
    DnaWriteArg = 1
End Function

Public Function DnaSelection() As String
    DnaSelection = TypeName(Selection)
End Function

Public Function DnaBook() As String
    DnaBook = ThisWorkbook.Name
End Function
