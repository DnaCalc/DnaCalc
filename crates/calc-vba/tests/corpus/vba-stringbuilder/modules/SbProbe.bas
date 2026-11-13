Attribute VB_Name = "SbProbe"
Option Explicit
' Own worksheet wrapper (MIT, this repository) around the verbatim StringBuilder class.

Public Function SbAppend(ByVal a As Variant, ByVal b As Variant) As String
    Dim sb As New StringBuilder
    sb.Append a
    sb.Append b
    SbAppend = sb.Peek & "|" & sb.Count
End Function

Public Function SbAll(ByVal a As Variant, ByVal b As Variant, ByVal c As Variant) As String
    Dim sb As New StringBuilder
    sb.AppendAll a, b, c
    SbAll = sb.Flush & "|" & sb.Count
End Function

Public Function SbRepeat(ByVal n As Long, ByVal s As String) As String
    Dim sb As New StringBuilder
    Dim i As Long
    For i = 1 To n
        sb.Append s
    Next i
    SbRepeat = sb.Count & "|" & Left$(sb.Peek, 5) & "|" & Right$(sb.Peek, 5)
End Function

Public Function SbShorten(ByVal s As String, ByVal cut As Long) As String
    Dim sb As New StringBuilder
    sb.Append s
    sb.Shorten cut
    SbShorten = sb.Peek & "|" & sb.Count
End Function

Public Function SbShortenDefault(ByVal s As String) As String
    Dim sb As New StringBuilder
    sb.Append s
    sb.Shorten
    SbShortenDefault = sb.Peek & "|" & sb.Count
End Function

Public Function SbMid(ByVal s As String, ByVal start As Long, ByVal length As Long) As String
    Dim sb As New StringBuilder
    sb.Append s
    SbMid = "[" & sb.Mid(start, length) & "]"
End Function

Public Function SbMidToEnd(ByVal s As String, ByVal start As Long) As String
    Dim sb As New StringBuilder
    sb.Append s
    SbMidToEnd = "[" & sb.Mid(start) & "]"
End Function

Public Function SbMidLet(ByVal s As String, ByVal start As Long, ByVal rhs As String) As String
    Dim sb As New StringBuilder
    sb.Append s
    sb.Mid(start) = rhs
    SbMidLet = sb.Peek
End Function

Public Function SbFlushTwice(ByVal s As String) As String
    Dim sb As New StringBuilder
    sb.Append s
    SbFlushTwice = sb.Flush & "|" & sb.Flush & "|" & sb.Count
End Function

Public Function SbClear(ByVal s As String) As String
    Dim sb As New StringBuilder
    sb.Append s
    sb.Clear
    sb.Append "x"
    SbClear = sb.Peek & "|" & sb.Count
End Function

Public Function SbEmptyAndMixed() As String
    Dim sb As New StringBuilder
    sb.Append ""
    sb.AppendAll "a", 1, 2.5, True, "", -7
    SbEmptyAndMixed = sb.Peek & "|" & sb.Count
End Function
