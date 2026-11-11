Attribute VB_Name = "DnaSimple"
Option Explicit

' Plain worksheet functions (dc-0f1k.4, dc-0f1k.10 push of 2026-11-11): numbers, text, logicals,
' Optional and ParamArray arguments, arrays in and out, CVErr and raised errors, Variant coercion
' and Application.WorksheetFunction. No object model beyond WorksheetFunction. Built into
' DnaExamples.xlam and DnaExamples.xlsm beside DnaExamples.bas by vba-job.ps1 -Action build; the
' cases are simple-udf-cases.json.

Public Function SimpleNeg(ByVal x As Double) As Double
    SimpleNeg = -x
End Function

Public Function SimpleHalf(ByVal x As Long) As Double
    SimpleHalf = x / 2
End Function

Public Function SimpleIsBig(ByVal x As Double) As Boolean
    SimpleIsBig = x > 100
End Function

Public Function SimpleNot(ByVal b As Boolean) As Boolean
    SimpleNot = Not b
End Function

Public Function SimpleUpper(ByVal s As String) As String
    SimpleUpper = UCase$(s)
End Function

Public Function SimpleLen(ByVal s As String) As Long
    SimpleLen = Len(s)
End Function

Public Function SimpleMid(ByVal s As String, ByVal start As Long, Optional ByVal count As Long = 2) As String
    SimpleMid = Mid$(s, start, count)
End Function

Public Function SimpleKind(ByVal v As Variant) As String
    SimpleKind = TypeName(v)
End Function

Public Function SimpleVarType(ByVal v As Variant) As Long
    SimpleVarType = VarType(v)
End Function

Public Function SimpleTwice(ByVal v As Variant) As Variant
    SimpleTwice = v + v
End Function

Public Function SimpleAsDouble(ByVal x As Double) As Double
    SimpleAsDouble = x
End Function

Public Function SimpleAsLong(ByVal x As Long) As Long
    SimpleAsLong = x
End Function

Public Function SimpleAsString(ByVal s As String) As String
    SimpleAsString = s
End Function

Public Function SimpleAsBool(ByVal b As Boolean) As String
    SimpleAsBool = CStr(b)
End Function

Public Function SimplePower(ByVal x As Double, Optional ByVal n As Double = 2) As Double
    SimplePower = x ^ n
End Function

Public Function SimpleMissing(Optional ByVal v As Variant) As Boolean
    SimpleMissing = IsMissing(v)
End Function

Public Function SimpleCount(ParamArray values() As Variant) As Long
    SimpleCount = UBound(values) - LBound(values) + 1
End Function

Public Function SimpleConcat(ByVal sep As String, ParamArray parts() As Variant) As String
    Dim i As Long, text As String
    For i = LBound(parts) To UBound(parts)
        If i > LBound(parts) Then text = text & sep
        text = text & CStr(parts(i))
    Next
    SimpleConcat = text
End Function

Public Function SimpleSeq(ByVal n As Long) As Variant
    Dim a() As Variant, i As Long
    ReDim a(1 To n, 1 To 1)
    For i = 1 To n
        a(i, 1) = i * 10
    Next
    SimpleSeq = a
End Function

Public Function SimpleRow(ByVal n As Long) As Variant
    Dim a() As Variant, i As Long
    ReDim a(0 To n - 1)
    For i = 0 To n - 1
        a(i) = i + 1
    Next
    SimpleRow = a
End Function

Public Function SimpleMixed() As Variant
    SimpleMixed = Array(1, "two", True, CVErr(2007))
End Function

Public Function SimpleGrid() As Variant
    Dim a(1 To 2, 1 To 3) As Double, r As Long, c As Long
    For r = 1 To 2
        For c = 1 To 3
            a(r, c) = r * 10 + c
        Next
    Next
    SimpleGrid = a
End Function

Public Function SimpleSplit(ByVal s As String) As Variant
    SimpleSplit = Split(s, ",")
End Function

Public Function SimpleArrSum(ByVal v As Variant) As Double
    Dim x As Variant, total As Double
    If IsArray(v) Then
        For Each x In v
            If VarType(x) = vbDouble Then total = total + x
        Next
    Else
        total = v
    End If
    SimpleArrSum = total
End Function

Public Function SimpleDims(ByVal v As Variant) As String
    Dim text As String
    text = TypeName(v)
    If IsArray(v) Then
        text = text & ":" & LBound(v, 1) & "-" & UBound(v, 1)
        On Error Resume Next
        Dim upper2 As Long
        upper2 = UBound(v, 2)
        If Err.Number = 0 Then text = text & "x" & LBound(v, 2) & "-" & upper2 Else text = text & ":1d"
    End If
    SimpleDims = text
End Function

Public Function SimpleCorner(ByVal v As Variant) As Variant
    SimpleCorner = v(UBound(v, 1), UBound(v, 2))
End Function

Public Function SimpleDiv(ByVal a As Double, ByVal b As Double) As Variant
    If b = 0 Then
        SimpleDiv = CVErr(xlErrDiv0)
    Else
        SimpleDiv = a / b
    End If
End Function

Public Function SimpleNum() As Variant
    SimpleNum = CVErr(xlErrNum)
End Function

Public Function SimpleErrName(ByVal v As Variant) As String
    If IsError(v) Then
        SimpleErrName = CStr(v)
    Else
        SimpleErrName = "ok"
    End If
End Function

Public Function SimpleOverflow(ByVal x As Long) As Long
    SimpleOverflow = x * 100000
End Function

Public Function SimpleRaise(ByVal n As Long) As Variant
    On Error GoTo failed
    Err.Raise n
    Exit Function
failed:
    SimpleRaise = Err.Number
End Function

Public Function SimpleEmpty() As Variant
End Function

Public Function SimpleBlankText() As String
    SimpleBlankText = ""
End Function

Public Function SimpleDate() As Date
    SimpleDate = DateSerial(2026, 11, 11)
End Function

Public Function SimpleMoney() As Currency
    SimpleMoney = 1.23456
End Function

Public Function SimpleRound(ByVal x As Double) As Double
    SimpleRound = Round(x, 1)
End Function

Public Function SimpleFormat(ByVal x As Double) As String
    SimpleFormat = Format$(x, "0.00")
End Function

Public Function SimpleGrade(ByVal score As Double) As String
    Select Case score
        Case Is >= 90
            SimpleGrade = "A"
        Case 70 To 89.999
            SimpleGrade = "B"
        Case Else
            SimpleGrade = "C"
    End Select
End Function

Public Function SimpleFact(ByVal n As Long) As Double
    If n <= 1 Then
        SimpleFact = 1
    Else
        SimpleFact = n * SimpleFact(n - 1)
    End If
End Function

Public Function SimpleWsMax(ByVal a As Double, ByVal b As Double, ByVal c As Double) As Double
    SimpleWsMax = Application.WorksheetFunction.Max(a, b, c)
End Function

Public Function SimpleWsMatch(ByVal s As String) As Variant
    On Error Resume Next
    SimpleWsMatch = Application.WorksheetFunction.Match(s, Array("a", "b", "c"), 0)
    If Err.Number <> 0 Then SimpleWsMatch = "Err " & Err.Number
End Function

Public Function SimpleWsMatchRaw(ByVal s As String) As Variant
    SimpleWsMatchRaw = Application.WorksheetFunction.Match(s, Array("a", "b", "c"), 0)
End Function

Private Function SimpleHidden(ByVal x As Double) As Double
    SimpleHidden = x
End Function
