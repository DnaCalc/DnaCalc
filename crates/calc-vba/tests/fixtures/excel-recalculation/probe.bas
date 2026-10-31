Option Explicit
Private calls(1 To 30) As Long

Public Function PlainProbe(ByVal ident As Long, x As Variant) As Long
    calls(ident) = calls(ident) + 1
    PlainProbe = calls(ident)
End Function
Public Function VolProbe(ByVal ident As Long, x As Variant) As Long
    Application.Volatile
    calls(ident) = calls(ident) + 1
    VolProbe = calls(ident)
End Function
Public Function ToggleProbe(ByVal ident As Long, ByVal flag As Boolean, x As Variant) As Long
    Application.Volatile flag
    calls(ident) = calls(ident) + 1
    ToggleProbe = calls(ident)
End Function
Public Function ConditionalProbe(ByVal ident As Long, ByVal flag As Boolean, x As Variant) As Long
    If flag Then Application.Volatile True
    calls(ident) = calls(ident) + 1
    ConditionalProbe = calls(ident)
End Function
Public Function LastWinsProbe(ByVal ident As Long, x As Variant) As Long
    Application.Volatile True
    Application.Volatile False
    calls(ident) = calls(ident) + 1
    LastWinsProbe = calls(ident)
End Function
Private Sub SetNestedVolatility(ByVal flag As Boolean)
    Application.Volatile flag
End Sub
Public Function NestedProbe(ByVal ident As Long, ByVal flag As Boolean, x As Variant) As Long
    SetNestedVolatility flag
    calls(ident) = calls(ident) + 1
    NestedProbe = calls(ident)
End Function
Public Function HiddenProbe() As String
    calls(12) = calls(12) + 1
    HiddenProbe = CStr(calls(12)) & ":" & CStr(ThisWorkbook.Worksheets("Probe").Range("Z1").Value2)
End Function
Public Function StableVolProbe(x As Variant) As Long
    Application.Volatile
    calls(16) = calls(16) + 1
    StableVolProbe = 0
End Function
Public Function NoArgsProbe() As Long
    Static n As Long
    n = n + 1
    NoArgsProbe = n
End Function
Private Function Snapshot(ByVal label As String, ws As Worksheet) As String
    Dim i As Long, text As String
    text = label & "="
    For i = 1 To 23
        If i > 1 Then text = text & ","
        text = text & CStr(ws.Cells(i, 2).Value2)
    Next i
    text = text & ";stable-calls=" & CStr(calls(16))
    Snapshot = text
End Function
Public Function RunProbe() As String
    On Error GoTo Failed
    Dim ws As Worksheet, other As Worksheet
    Dim oldCalc As Long, text As String, stage As String
    stage = "setup"
    Set ws = ThisWorkbook.Worksheets(1)
    ws.Name = "Probe"
    Set other = ThisWorkbook.Worksheets.Add
    oldCalc = Application.Calculation
    Application.Calculation = xlCalculationManual
    ws.Range("A1").Value2 = 10
    ws.Range("A3").Formula2 = "=A1*0"
    ws.Range("A4").Formula2 = "=RAND()*0"
    ws.Range("A6").Formula2 = "=RAND()"
    ws.Range("D1").Value2 = True
    ws.Range("D2").Value2 = False
    ws.Range("D3").Value2 = True
    ws.Range("D4").Value2 = False
    ws.Range("Z1").Value2 = 30
    ws.Range("B1").Formula2 = "=PlainProbe(1,A1)"
    ws.Range("B2").Formula2 = "=VolProbe(2,A1)"
    ws.Range("B3").Formula2 = "=PlainProbe(3,A3)"
    ws.Range("B4").Formula2 = "=PlainProbe(4,1)"
    ws.Range("B5").Formula2 = "=PlainProbe(5,A4)"
    ws.Range("B6").Formula2 = "=ToggleProbe(6,D1,A1)"
    ws.Range("B7").Formula2 = "=ToggleProbe(7,D2,A1)"
    ws.Range("B8").Formula2 = "=ConditionalProbe(8,D3,A1)"
    ws.Range("B9").Formula2 = "=LastWinsProbe(9,A1)"
    ws.Range("B10").Formula2 = "=NestedProbe(10,D4,A1)"
    ws.Range("B11").Formula2 = "=PlainProbe(11,B2*0)"
    ws.Range("B12").Formula2 = "=HiddenProbe()"
    ws.Range("B13").Formula2 = "=NoArgsProbe()"
    ws.Range("B14").Formula2 = "=PlainProbe(14,RAND()*0)"
    ws.Range("B15").Formula2 = "=PlainProbe(15,A6)"
    ws.Range("B16").Formula2 = "=StableVolProbe(A1)"
    ws.Range("B17").Formula2 = "=PlainProbe(17,B16)"
    ws.Range("B18").Formula2 = "=ToggleProbe(18,D1,A1)+ToggleProbe(19,D2,A1)"
    ws.Range("B19").Formula2 = "=ToggleProbe(20,D2,A1)+ToggleProbe(21,D1,A1)"
    ws.Range("B20").Formula2 = "=VolProbe(22,A1)+PlainProbe(23,A1)"
    ws.Range("B21").Formula2 = "=PlainProbe(28,1)"
    ws.Range("B22").Formula2 = "=IF(TRUE,PlainProbe(24,A1),VolProbe(25,A1))"
    ws.Range("B23").Formula2 = "=IF(TRUE,VolProbe(26,A1),PlainProbe(27,A1))"
    stage = "baseline"
    Application.Calculate
    text = Snapshot(stage, ws)
    stage = "clean"
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "unrelated"
    ws.Range("A2").Value2 = 99
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "same-input"
    ws.Range("A1").Value2 = 10
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "changed-input"
    ws.Range("A1").Value2 = 11
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "range-force"
    ws.Range("B1").Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "sheet-clean"
    ws.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "full"
    Application.CalculateFull
    text = text & "|" & Snapshot(stage, ws)
    stage = "rebuild"
    Application.CalculateFullRebuild
    text = text & "|" & Snapshot(stage, ws)
    stage = "toggle"
    ws.Range("D1").Value2 = False
    ws.Range("D2").Value2 = True
    ws.Range("D3").Value2 = False
    ws.Range("D4").Value2 = True
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "toggle-clean"
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "outside-volatile"
    Application.Volatile True
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "hidden-input"
    ws.Range("Z1").Value2 = 31
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "other-sheet"
    other.Range("A1").Value2 = 77
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "hidden-force"
    ws.Range("B12").Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "toggle-back"
    ws.Range("D1").Value2 = True
    ws.Range("D2").Value2 = False
    ws.Range("D4").Value2 = False
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    stage = "back-clean"
    Application.Calculate
    text = text & "|" & Snapshot(stage, ws)
    Application.Calculation = oldCalc
    RunProbe = text
    Exit Function
Failed:
    RunProbe = "fault:" & CStr(Err.Number) & ":" & Err.Description & ":" & stage & ":" & text
End Function
