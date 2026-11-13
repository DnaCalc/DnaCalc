Attribute VB_Name = "KcProbe"
Option Explicit
' Own worksheet wrapper (MIT, this repository) around the verbatim KeyedCollection class.

Private Function Fill() As KeyedCollection
    Dim kc As New KeyedCollection
    kc.Add "a", 1
    kc.Add "b", "two"
    kc.Add "c", 3.5
    Set Fill = kc
End Function

Private Function Joined(ByVal v As Variant) As String
    Dim i As Long
    Dim s As String
    If UBound(v) < LBound(v) Then
        Joined = "(empty)"
        Exit Function
    End If
    For i = LBound(v) To UBound(v)
        s = s & CStr(v(i)) & ";"
    Next i
    Joined = s
End Function

Public Function KcBasic() As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    KcBasic = kc.Count & "|" & kc.Item("b") & "|" & kc.Item(1) & "|" & kc.Item(3) & "|" & kc.Exists("a") & "|" & kc.Exists("zz")
End Function

Public Function KcKeys() As String
    KcKeys = Joined(Fill().Keys)
End Function

Public Function KcItems() As String
    KcItems = Joined(Fill().Items)
End Function

Public Function KcEmpty() As String
    Dim kc As New KeyedCollection
    KcEmpty = Joined(kc.Keys) & "|" & Joined(kc.Items) & "|" & kc.Count
End Function

Public Function KcRemoveKey(ByVal key As String) As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    kc.Remove key
    KcRemoveKey = Joined(kc.Keys) & "|" & kc.Count
End Function

Public Function KcRemoveIndex(ByVal idx As Long) As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    kc.Remove idx
    KcRemoveIndex = Joined(kc.Keys) & "|" & Joined(kc.Items)
End Function

Public Function KcRemoveAll() As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    kc.RemoveAll
    kc.Add "n", 1
    KcRemoveAll = Joined(kc.Keys) & "|" & kc.Count
End Function

Public Function KcUpdate() As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    kc.Item("b") = 20
    kc.Item(3) = "three"
    KcUpdate = Joined(kc.Keys) & "|" & Joined(kc.Items)
End Function

Public Function KcRename() As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    kc.Key("b") = "z"
    KcRename = Joined(kc.Keys) & "|" & Joined(kc.Items) & "|" & kc.Exists("b") & "|" & kc.Exists("z")
End Function

Public Function KcInsertBefore() As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    kc.Add "x", 9, "c"
    KcInsertBefore = Joined(kc.Keys) & "|" & Joined(kc.Items)
End Function

Public Function KcInsertAfter() As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    kc.Add "x", 9, , "a"
    KcInsertAfter = Joined(kc.Keys) & "|" & Joined(kc.Items)
End Function

Public Function KcCaseInsensitive() As String
    Dim kc As New KeyedCollection
    kc.Add "Abc", 7
    KcCaseInsensitive = kc.Exists("aBC") & "|" & kc.Item("ABC") & "|" & kc.CompareMode
End Function

Public Function KcDuplicateKey() As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    kc.Add "A", 99
    KcDuplicateKey = "no error"
End Function

Public Function KcMissingKey() As String
    KcMissingKey = Fill().Item("nope")
End Function

Public Function KcPairs() As String
    Dim p As Variant
    p = Fill().KeyItemPairs
    KcPairs = UBound(p, 1) & "x" & UBound(p, 2) & "|" & p(1, 1) & "=" & p(1, 2) & "|" & p(3, 1) & "=" & p(3, 2)
End Function

Public Function KcKeyAtIndex(ByVal idx As Long) As String
    KcKeyAtIndex = Fill().GetKeyAtIndex(idx)
End Function

Public Function KcObjectItem() As String
    Dim kc As New KeyedCollection
    Dim c As New Collection
    c.Add 5
    c.Add 6
    kc.Add "k", 0
    Set kc.Item("k") = c
    Set kc.Item("k") = c
    KcObjectItem = kc.Count
End Function

Public Function KcLetObjectError() As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    On Error Resume Next
    kc.Item("a") = New Collection
    KcLetObjectError = Err.Number & "|" & Err.Description
End Function

Public Function KcSelf() As String
    Dim kc As KeyedCollection
    Set kc = Fill()
    KcSelf = kc.Self.Count & "|" & TypeName(kc.Self)
End Function
