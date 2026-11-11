Attribute VB_Name = "OracleProbesCF"
' Excel/VBA control-flow oracle probes: GoTo, GoSub/Return, On..GoTo/GoSub, For/Next
' counter quirks, Exit, Select Case, Do/While. Each PROBE_* returns a result string;
' every loop is bounded so a probe cannot spin.

' ============================ GoTo ============================

Function PROBE_cf_goto_forward() As String
    Dim r As String
    r = "a;"
    GoTo L
    r = r & "skipped;"
L:
    PROBE_cf_goto_forward = r & "b;"
End Function

Function PROBE_cf_goto_backward() As String
    Dim r As String, i As Long
    i = 0
Top:
    i = i + 1
    r = r & i & ";"
    If i < 3 Then GoTo Top
    PROBE_cf_goto_backward = r
End Function

Function PROBE_cf_goto_out_of_for() As String
    Dim i As Long, s As String
    For i = 1 To 10
        s = s & i & ";"
        If i = 3 Then GoTo Done
    Next i
Done:
    PROBE_cf_goto_out_of_for = "s=" & s & "i=" & i
End Function

' ============================ For/Next counter quirks ============================

Function PROBE_cf_for_counter_after() As String
    Dim i As Long, s As String
    For i = 1 To 3
        s = s & i & ";"
    Next i
    PROBE_cf_for_counter_after = "loop=" & s & "final_i=" & i
End Function

Function PROBE_cf_for_step() As String
    Dim i As Long, s As String
    For i = 1 To 10 Step 3
        s = s & i & ";"
    Next i
    PROBE_cf_for_step = "loop=" & s & "final_i=" & i
End Function

Function PROBE_cf_for_step_neg() As String
    Dim i As Long, s As String
    For i = 5 To 1 Step -2
        s = s & i & ";"
    Next i
    PROBE_cf_for_step_neg = "loop=" & s & "final_i=" & i
End Function

Function PROBE_cf_for_never_runs() As String
    Dim i As Long, s As String
    s = "x"
    For i = 5 To 1            ' Step defaults to 1; 5 > 1 so zero iterations
        s = s & i & ";"
    Next i
    PROBE_cf_for_never_runs = "s=" & s & ";final_i=" & i
End Function

Function PROBE_cf_for_bound_evaluated_once() As String
    Dim i As Long, n As Long, s As String
    n = 3
    For i = 1 To n
        n = 10               ' does the loop limit re-read n (run to 10) or stay 3?
        s = s & i & ";"
    Next i
    PROBE_cf_for_bound_evaluated_once = "loop=" & s & "final_i=" & i
End Function

Function PROBE_cf_for_modify_counter() As String
    Dim i As Long, s As String
    For i = 1 To 5
        i = i + 1            ' bump the counter inside the body
        s = s & i & ";"
    Next i
    PROBE_cf_for_modify_counter = "loop=" & s & "final_i=" & i
End Function

Function PROBE_cf_exit_for() As String
    Dim i As Long, s As String
    For i = 1 To 10
        If i = 3 Then Exit For
        s = s & i & ";"
    Next i
    PROBE_cf_exit_for = "s=" & s & "final_i=" & i
End Function

Function PROBE_cf_nested_for_exit() As String
    Dim i As Long, j As Long, s As String
    For i = 1 To 2
        For j = 1 To 5
            If j = 2 Then Exit For    ' exits the INNER loop only
            s = s & i & "." & j & ";"
        Next j
    Next i
    PROBE_cf_nested_for_exit = "s=" & s & "i=" & i & ";j=" & j
End Function

' ============================ GoSub / Return ============================

Function PROBE_cf_gosub_return() As String
    Dim r As String
    r = "a;"
    GoSub Subr
    r = r & "c;"
    PROBE_cf_gosub_return = r
    Exit Function
Subr:
    r = r & "b;"
    Return
End Function

Function PROBE_cf_gosub_twice() As String
    Dim r As String
    GoSub Subr
    GoSub Subr
    PROBE_cf_gosub_twice = r
    Exit Function
Subr:
    r = r & "x;"
    Return
End Function

' ============================ On N GoTo / GoSub ============================

Function PROBE_cf_on_n_goto() As String
    Dim r As String, n As Long
    n = 2
    On n GoTo L1, L2, L3
    r = "fell;"
    GoTo Done
L1:
    r = "L1;": GoTo Done
L2:
    r = "L2;": GoTo Done
L3:
    r = "L3;"
Done:
    PROBE_cf_on_n_goto = r
End Function

Function PROBE_cf_on_n_goto_zero() As String
    Dim r As String, n As Long
    n = 0                    ' 0 => no branch, fall through
    On n GoTo L1, L2
    r = "fell;"
    GoTo Done
L1:
    r = "L1;": GoTo Done
L2:
    r = "L2;"
Done:
    PROBE_cf_on_n_goto_zero = r
End Function

Function PROBE_cf_on_n_gosub() As String
    Dim r As String, n As Long
    n = 2
    On n GoSub S1, S2
    PROBE_cf_on_n_gosub = r & "end"
    Exit Function
S1:
    r = "S1;": Return
S2:
    r = "S2;": Return
End Function

' ============================ Select Case ============================

Function PROBE_cf_select_case() As String
    Dim x As Long, r As String
    x = 7
    Select Case x
        Case 1, 2, 3
            r = "low"
        Case 4 To 9
            r = "mid"
        Case Is > 100
            r = "huge"
        Case Else
            r = "other"
    End Select
    PROBE_cf_select_case = "r=" & r
End Function

Function PROBE_cf_select_first_match_wins() As String
    Dim x As Long, r As String
    x = 5
    Select Case x
        Case 1 To 10
            r = "A"
        Case 5
            r = "B"          ' also matches, but the first matching Case wins
    End Select
    PROBE_cf_select_first_match_wins = "r=" & r
End Function

' ============================ Do / While ============================

Function PROBE_cf_do_while() As String
    Dim i As Long, s As String
    i = 0
    Do While i < 3
        i = i + 1
        s = s & i & ";"
    Loop
    PROBE_cf_do_while = "s=" & s & "i=" & i
End Function

Function PROBE_cf_do_loop_until() As String
    Dim i As Long, s As String
    i = 0
    Do
        i = i + 1
        s = s & i & ";"
    Loop Until i >= 3
    PROBE_cf_do_loop_until = "s=" & s & "i=" & i
End Function

Function PROBE_cf_exit_do() As String
    Dim i As Long, s As String
    i = 0
    Do
        i = i + 1
        If i = 3 Then Exit Do
        s = s & i & ";"
    Loop While i < 100
    PROBE_cf_exit_do = "s=" & s & "i=" & i
End Function

Function PROBE_cf_while_wend() As String
    Dim i As Long, s As String
    i = 0
    While i < 3
        i = i + 1
        s = s & i & ";"
    Wend
    PROBE_cf_while_wend = "s=" & s & "i=" & i
End Function

' ============================ For Each ============================

Function PROBE_cf_for_each_array() As String
    Dim v As Variant, arr As Variant, s As String
    arr = Array(10, 20, 30)
    For Each v In arr
        s = s & v & ";"
    Next v
    PROBE_cf_for_each_array = "s=" & s
End Function
