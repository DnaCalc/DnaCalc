Attribute VB_Name = "DictProbe"
Option Explicit

' Own code (MIT, this repository): worksheet entry points over VBA-Dictionary. Pure functions only.

Private Function Build(ByVal csv As String, ByVal textCompare As Boolean) As Dictionary
    Dim d As New Dictionary
    Dim parts() As String, i As Long, p As Long
    If textCompare Then d.CompareMode = TextCompare
    parts = Split(csv, ",")
    For i = LBound(parts) To UBound(parts)
        p = InStr(parts(i), "=")
        If p > 0 Then d(Left$(parts(i), p - 1)) = Mid$(parts(i), p + 1)
    Next i
    Set Build = d
End Function

Public Function DictCount(ByVal csv As String) As Long
    DictCount = Build(csv, False).Count
End Function

Public Function DictLookup(ByVal csv As String, ByVal k As String) As Variant
    DictLookup = Build(csv, False)(k)
End Function

Public Function DictLookupText(ByVal csv As String, ByVal k As String) As Variant
    DictLookupText = Build(csv, True)(k)
End Function

Public Function DictExists(ByVal csv As String, ByVal k As String) As Boolean
    DictExists = Build(csv, False).Exists(k)
End Function

Public Function DictKeysJoined(ByVal csv As String) As String
    DictKeysJoined = Join(Build(csv, False).Keys, "|")
End Function

Public Function DictItemsJoined(ByVal csv As String) As String
    DictItemsJoined = Join(Build(csv, False).Items, "|")
End Function

Public Function DictAfterRemove(ByVal csv As String, ByVal k As String) As String
    Dim d As Dictionary
    Set d = Build(csv, False)
    d.Remove k
    DictAfterRemove = Join(d.Keys, "|")
End Function
