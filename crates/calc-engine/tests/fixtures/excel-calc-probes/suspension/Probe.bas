Attribute VB_Name = "Probe"
Option Explicit
' Evaluator-suspension probe (eval-1). Tk(tag, v) records each call in order and returns v
' unchanged; nothing else is read or written inside a UDF.
Private seq As String
Private n As Long

Public Function Tk(ByVal tag As String, Optional ByVal v As Variant) As Variant
    n = n + 1
    Dim s As String
    If IsMissing(v) Then
        s = "missing"
    ElseIf IsObject(v) Then
        s = "object"
    ElseIf IsError(v) Then
        s = "error"
    ElseIf IsEmpty(v) Then
        s = "empty"
    ElseIf VarType(v) = vbDouble Then
        s = Replace(Format$(v, "0.000000000000000E+00"), ",", ".")
    ElseIf VarType(v) = vbDate Then
        s = Replace(Format$(CDbl(v), "0.000000000000000E+00"), ",", ".")
    Else
        s = CStr(v)
    End If
    seq = seq & n & ":" & tag & "=" & s & "@" & Application.Caller.Address(False, False) & ";"
    Tk = v
End Function

Public Sub ResetSeq()
    seq = ""
    n = 0
End Sub

Public Function GetSeq() As String
    GetSeq = seq
End Function
