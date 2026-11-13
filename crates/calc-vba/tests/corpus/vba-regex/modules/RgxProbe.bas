Attribute VB_Name = "RgxProbe"
Option Explicit
' Own worksheet wrapper (MIT, this repository) around the verbatim VbaRegex engine (StaticRegex API).

Public Function RgxValid(ByVal pattern As String) As Boolean
    Dim re As StaticRegex.RegexTy
    RgxValid = StaticRegex.TryInitializeRegex(re, pattern)
End Function

Public Function RgxTest(ByVal pattern As String, ByVal text As String, Optional ByVal ignoreCase As Boolean = False) As Boolean
    Dim re As StaticRegex.RegexTy
    StaticRegex.InitializeRegex re, pattern, ignoreCase
    RgxTest = StaticRegex.Test(re, text)
End Function

Public Function RgxTestMultiline(ByVal pattern As String, ByVal text As String) As Boolean
    Dim re As StaticRegex.RegexTy
    StaticRegex.InitializeRegex re, pattern
    RgxTestMultiline = StaticRegex.Test(re, text, True)
End Function

Public Function RgxFind(ByVal pattern As String, ByVal text As String) As String
    Dim re As StaticRegex.RegexTy
    Dim st As StaticRegex.MatcherStateTy
    StaticRegex.InitializeRegex re, pattern
    If StaticRegex.Match(st, re, text) Then
        RgxFind = "[" & StaticRegex.GetCapture(st, text) & "]"
    Else
        RgxFind = "no match"
    End If
End Function

Public Function RgxGroup(ByVal pattern As String, ByVal text As String, ByVal n As Long) As String
    Dim re As StaticRegex.RegexTy
    Dim st As StaticRegex.MatcherStateTy
    StaticRegex.InitializeRegex re, pattern
    If StaticRegex.Match(st, re, text) Then
        RgxGroup = "[" & StaticRegex.GetCapture(st, text, n) & "]"
    Else
        RgxGroup = "no match"
    End If
End Function

Public Function RgxNamed(ByVal pattern As String, ByVal text As String, ByVal name As String) As String
    Dim re As StaticRegex.RegexTy
    Dim st As StaticRegex.MatcherStateTy
    StaticRegex.InitializeRegex re, pattern
    If StaticRegex.Match(st, re, text) Then
        RgxNamed = "[" & StaticRegex.GetCaptureByName(st, re, text, name) & "]"
    Else
        RgxNamed = "no match"
    End If
End Function

Public Function RgxReplace(ByVal pattern As String, ByVal replacement As String, ByVal text As String) As String
    Dim re As StaticRegex.RegexTy
    StaticRegex.InitializeRegex re, pattern
    RgxReplace = StaticRegex.Replace(re, replacement, text)
End Function

Public Function RgxSplit(ByVal pattern As String, ByVal text As String) As String
    Dim re As StaticRegex.RegexTy
    Dim parts As Collection
    Dim v As Variant
    Dim s As String
    StaticRegex.InitializeRegex re, pattern
    Set parts = StaticRegex.SplitByRegex(re, text)
    For Each v In parts
        s = s & "<" & v & ">"
    Next v
    RgxSplit = parts.Count & ":" & s
End Function

Public Function RgxJoin(ByVal pattern As String, ByVal text As String, ByVal fmt As String, ByVal delim As String) As String
    Dim re As StaticRegex.RegexTy
    StaticRegex.InitializeRegex re, pattern
    RgxJoin = StaticRegex.MatchThenJoin(re, text, fmt, delim)
End Function

Public Function RgxCount(ByVal pattern As String, ByVal text As String) As Long
    Dim re As StaticRegex.RegexTy
    Dim st As StaticRegex.MatcherStateTy
    StaticRegex.InitializeRegex re, pattern
    Do While StaticRegex.MatchNext(st, re, text)
        RgxCount = RgxCount + 1
    Loop
End Function
