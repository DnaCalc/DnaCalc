Attribute VB_Name = "OracleProbes"
' Excel/VBA error-handling oracle probes. Each PROBE_* is a self-contained
' Function As String that captures its own observable (and Err state) and returns a
' result string, so it never leaks an UNHANDLED run-time error to the COM boundary
' (which would pop the modal "Run-time error" dialog and wedge automation). Helper
' subs are per-probe (unique names) with Static guards where re-entry is possible, so
' a surprising oracle behaviour can never infinite-loop.

' ============================ A. On Error statement ============================

Function PROBE_oe_resume_next() As String
    Dim n As Long
    On Error Resume Next
    n = 1 / 0
    PROBE_oe_resume_next = "errnum=" & Err.Number & ";desc=" & Err.Description
End Function

Function PROBE_oe_goto_label() As String
    On Error GoTo H
    Err.Raise 13
    PROBE_oe_goto_label = "no-fault"
    Exit Function
H:
    PROBE_oe_goto_label = "handler;errnum=" & Err.Number
End Function

Function PROBE_oe_goto0_disables() As String
    Dim x As Long
    On Error Resume Next
    x = H_goto0()              ' the sub disables its handler then faults -> propagates here
    PROBE_oe_goto0_disables = "caught=" & Err.Number
End Function
Private Function H_goto0() As Long
    On Error Resume Next
    On Error GoTo 0           ' disable
    Err.Raise 7              ' unhandled in this proc -> propagates to caller
    H_goto0 = -1
End Function

Function PROBE_oe_resume_next_resets_err() As String
    Dim a As Long, b As Long
    On Error Resume Next
    Err.Raise 9
    a = Err.Number            ' 9
    On Error Resume Next      ' re-arm: does an On Error statement reset Err?
    b = Err.Number            ' 0 if reset
    PROBE_oe_resume_next_resets_err = "after_raise=" & a & ";after_rearm=" & b
End Function

Function PROBE_err_reset_on_onerror0() As String
    Dim a As Long, b As Long
    On Error Resume Next
    Err.Raise 5
    a = Err.Number
    On Error GoTo 0
    b = Err.Number
    PROBE_err_reset_on_onerror0 = "after_raise=" & a & ";after_onerror0=" & b
End Function

Function PROBE_oe_goto_minus1() As String
    Dim r As String
    On Error GoTo H
    Err.Raise 11
    r = r & "p2;"
    Err.Raise 13
    r = r & "p3;"
    PROBE_oe_goto_minus1 = r & "done"
    Exit Function
H:
    Static stage As Long
    stage = stage + 1
    If stage > 4 Then PROBE_oe_goto_minus1 = r & "GUARD": Exit Function
    r = r & "H" & stage & "(" & Err.Number & ");"
    On Error GoTo -1          ' clear the active error so the handler can re-catch
    On Error GoTo H
    Resume Next
End Function

' ============================ B. Resume statement ============================

Function PROBE_resume_without_error() As String
    On Error Resume Next
    Resume
    PROBE_resume_without_error = "errnum=" & Err.Number
End Function

Function PROBE_resume_reruns_faulting_stmt() As String
    Dim d As Long, n As Long, x As Long
    d = 0: n = 0
    On Error GoTo H
Again:
    n = n + 1
    x = 100 \ d              ' integer divide; faults when d=0; Resume re-runs THIS stmt
    PROBE_resume_reruns_faulting_stmt = "x=" & x & ";n=" & n
    Exit Function
H:
    d = 2
    Resume
End Function

Function PROBE_resume_next_continues() As String
    Dim r As String
    On Error GoTo H
    r = r & "before;"
    Err.Raise 5
    r = r & "after;"
    PROBE_resume_next_continues = r & "errnum=" & Err.Number
    Exit Function
H:
    r = r & "handler;"
    Resume Next
End Function

Function PROBE_resume_label() As String
    Dim r As String
    On Error GoTo H
    r = r & "before;"
    Err.Raise 5
    r = r & "skipped;"
Cont:
    PROBE_resume_label = r & "errnum=" & Err.Number
    Exit Function
H:
    r = r & "handler;"
    Resume Cont
End Function

Function PROBE_resume_clears_err() As String
    Dim seenInHandler As Long, afterResume As Long
    On Error GoTo H
    Err.Raise 5
    afterResume = Err.Number
    PROBE_resume_clears_err = "in_handler=" & seenInHandler & ";after_resume_next=" & afterResume
    Exit Function
H:
    seenInHandler = Err.Number
    Resume Next
End Function

' ============================ C. Err object ============================

Function PROBE_err_raise_full() As String
    On Error GoTo H
    Err.Raise 5, "MySrc", "MyDesc"
    Exit Function
H:
    PROBE_err_raise_full = "n=" & Err.Number & ";src=" & Err.Source & ";desc=" & Err.Description
End Function

Function PROBE_err_description_derivation() As String
    On Error GoTo H
    Err.Raise 11
    Exit Function
H:
    PROBE_err_description_derivation = "desc=" & Err.Description
End Function

Function PROBE_err_source_default() As String
    On Error GoTo H
    Err.Raise 5
    Exit Function
H:
    PROBE_err_source_default = "src=[" & Err.Source & "]"
End Function

Function PROBE_err_raise_omitted_inherit() As String
    ' MS-VBAL 9071: Raise with omitted args reuses un-cleared Err fields.
    On Error Resume Next
    Err.Raise 5, "Src1", "Desc1"
    On Error GoTo H
    Err.Raise 6              ' omit Source/Description -> inherit Src1/Desc1, or reset?
    Exit Function
H:
    PROBE_err_raise_omitted_inherit = "n=" & Err.Number & ";src=" & Err.Source & ";desc=" & Err.Description
End Function

Function PROBE_err_clear() As String
    On Error Resume Next
    Err.Raise 5
    Dim a As Long: a = Err.Number
    Err.Clear
    PROBE_err_clear = "before=" & a & ";after_clear=" & Err.Number
End Function

Function PROBE_err_persists_after_clean_stmt() As String
    Dim z As Long
    On Error Resume Next
    Err.Raise 5
    z = 1 + 1               ' a clean (non-faulting) statement
    PROBE_err_persists_after_clean_stmt = "errnum=" & Err.Number
End Function

' ============================ D. Propagation / active error ============================

Function PROBE_prop_callee_to_caller() As String
    On Error GoTo H
    Call Sub_NoHandlerFaults
    PROBE_prop_callee_to_caller = "no-fault"
    Exit Function
H:
    PROBE_prop_callee_to_caller = "caller-caught;errnum=" & Err.Number
End Function
Private Sub Sub_NoHandlerFaults()
    Err.Raise 6
End Sub

Function PROBE_reraise_in_handler_propagates() As String
    On Error GoTo CallerH
    Call Sub_Reraise
    PROBE_reraise_in_handler_propagates = "no-fault"
    Exit Function
CallerH:
    PROBE_reraise_in_handler_propagates = "caller-caught;errnum=" & Err.Number
End Function
Private Sub Sub_Reraise()
    Static depth As Long
    On Error GoTo InnerH
    Err.Raise 11
    Exit Sub
InnerH:
    depth = depth + 1
    If depth > 3 Then Exit Sub        ' guard against unexpected re-entry
    Err.Raise 5                       ' re-raise in handler -> propagate to caller?
End Sub

' NOTE: these two probes do Resume/Resume Next after a CROSS-CALL propagated error.
' That logic must NOT run in the frame Application.Run invokes directly (the COM top
' frame) or Excel crashes (a COM-invocation artifact, not real VBA). So each probe just
' delegates to an inner Private Function, which runs one frame down where the pattern
' behaves normally.

Function PROBE_resume_next_after_prop() As String
    PROBE_resume_next_after_prop = Inner_ResumeNextAfterProp()
End Function
Private Function Inner_ResumeNextAfterProp() As String
    Dim r As String
    On Error GoTo H
    r = r & "before;"
    Call Sub_FaultsUnlessFixed(0)        ' faults in the callee -> propagates to H
    r = r & "after;"                      ' Resume Next lands here (statement after the call)
    Inner_ResumeNextAfterProp = r & "errnum=" & Err.Number
    Exit Function
H:
    r = r & "handler;"
    Resume Next
End Function
Private Sub Sub_FaultsUnlessFixed(ByVal doFix As Long)
    ' NB: do NOT name this parameter `fix` -- `Fix` is a VBA intrinsic, so a `fix`
    ' parameter makes this Sub fail to compile and callers report "Sub or Function not
    ' defined".
    If doFix = 0 Then Err.Raise 5
End Sub

' DEGENERATE / EXCLUDED: bare `Resume` (re-run the faulting statement) after a
' CROSS-CALL propagated error reproducibly wedges/crashes Excel itself -- even when the
' handler is one frame down (unlike `Resume Next`, which works; see
' PROBE_resume_next_after_prop). This is Excel instability, not a behaviour to mirror,
' so it is documented in the findings and kept out of the runnable corpus. (Same-frame
' `Resume` re-run is fine and covered by PROBE_resume_reruns_faulting_stmt.)

Function PROBE_nested_onerror_restore() As String
    On Error GoTo CallerH
    Call Sub_SetsHandlerAndReturns
    Err.Raise 42                     ' should hit CallerH (the caller's handler is restored)
    PROBE_nested_onerror_restore = "no-fault"
    Exit Function
CallerH:
    PROBE_nested_onerror_restore = "caller-caught;errnum=" & Err.Number
End Function
Private Sub Sub_SetsHandlerAndReturns()
    On Error Resume Next             ' a different handler; must not leak to the caller
    Dim x As Long: x = 1
End Sub

' ============================ E. Exit vs end + Err ============================

Function PROBE_exit_sub_clears_err() As String
    On Error Resume Next
    Call Sub_RaiseThenExit
    PROBE_exit_sub_clears_err = "after_call_errnum=" & Err.Number
End Function
Private Sub Sub_RaiseThenExit()
    On Error Resume Next
    Err.Raise 5
    Exit Sub                         ' does Exit Sub clear Err?
End Sub

Function PROBE_end_sub_err_persists() As String
    On Error Resume Next
    Call Sub_RaiseThenEnd
    PROBE_end_sub_err_persists = "after_call_errnum=" & Err.Number
End Function
Private Sub Sub_RaiseThenEnd()
    On Error Resume Next
    Err.Raise 5
End Sub                              ' normal end (no Exit) -> does it clear Err?

' ===================== F. Err.Raise S9071 omitted-arg inheritance =====================
' MS-VBAL S6.1.3.2.1.2/S9071: an omitted Source/Description on Err.Raise INHERITS the
' un-cleared Err field (oracle-confirmed). Isolation vs `err_raise_omitted_inherit`
' above: NO intervening On Error between the two raises (an On Error statement resets
' Err). A single top-level On Error Resume Next catches both; its implicit skip does NOT
' clear Err, so the second raise sees the first's un-cleared fields.

Function PROBE_err_raise_inherit_noreset() As String
    On Error Resume Next
    Err.Raise 5, "Src1", "Desc1"      ' Err = {5, Src1, Desc1}
    Err.Raise 6                        ' omit Source+Description; Err un-cleared -> inherit
    PROBE_err_raise_inherit_noreset = "n=" & Err.Number & ";src=" & Err.Source & ";desc=" & Err.Description
End Function

Function PROBE_err_raise_inherit_partial_src() As String
    On Error Resume Next
    Err.Raise 5, "Src1", "Desc1"
    Err.Raise 6, "Src2"                ' new Source, omit Description -> Description inherits
    PROBE_err_raise_inherit_partial_src = "n=" & Err.Number & ";src=" & Err.Source & ";desc=" & Err.Description
End Function

Function PROBE_err_raise_inherit_same_num() As String
    On Error Resume Next
    Err.Raise 5, "Src1", "Desc1"
    Err.Raise 5                        ' same number, omit both -> inherit
    PROBE_err_raise_inherit_same_num = "n=" & Err.Number & ";src=" & Err.Source & ";desc=" & Err.Description
End Function

Function PROBE_err_raise_inherit_after_clear() As String
    On Error Resume Next
    Err.Raise 5, "Src1", "Desc1"
    Err.Clear                          ' Err cleared -> omitted args fall to defaults
    Err.Raise 6
    PROBE_err_raise_inherit_after_clear = "n=" & Err.Number & ";src=" & Err.Source & ";desc=" & Err.Description
End Function

Function PROBE_err_source_system() As String
    On Error Resume Next
    Dim x As Long
    x = 1 / 0                          ' system error: Err.Source = project name (not "")
    PROBE_err_source_system = "n=" & Err.Number & ";src=[" & Err.Source & "]"
End Function

Function PROBE_err_system_after_raise() As String
    On Error Resume Next
    Err.Raise 5, "Src1", "Desc1"       ' Err = {5, Src1, Desc1}
    Dim x As Long: x = 1 / 0           ' system error sets FRESH fields (no S9071 inherit)
    PROBE_err_system_after_raise = "n=" & Err.Number & ";src=" & Err.Source & ";desc=" & Err.Description
End Function

Function PROBE_err_raise_inherit_after_system() As String
    On Error Resume Next
    Dim x As Long: x = 1 / 0           ' Err = {11, <project>, "Division by zero"}
    Err.Raise 6                        ' omit -> inherits the system error's un-cleared Err
    PROBE_err_raise_inherit_after_system = "n=" & Err.Number & ";src=" & Err.Source & ";desc=" & Err.Description
End Function

' ===================== G. legacy `Error <n>` statement =====================
' MS-VBAL S2841 says `Error <n>` is "as if Err.Raise(number)", but the ORACLE shows it
' does NOT do S9071 inheritance -- it sets FRESH fields (like a system error), unlike
' Err.Raise. So `Error <n>` is a non-inheriting raise.

Function PROBE_error_statement() As String
    On Error GoTo H
    Error 5
    PROBE_error_statement = "no-fault"
    Exit Function
H:
    PROBE_error_statement = "handler;errnum=" & Err.Number & ";desc=" & Err.Description
End Function

Function PROBE_error_statement_inherit() As String
    On Error Resume Next
    Err.Raise 5, "Src1", "Desc1"       ' Err = {5, Src1, Desc1}
    Error 6                            ' Error stmt does NOT inherit -> fresh defaults
    PROBE_error_statement_inherit = "n=" & Err.Number & ";src=" & Err.Source & ";desc=" & Err.Description
End Function

Function PROBE_error_statement_clean() As String
    On Error Resume Next
    Error 11                           ' from clean Err -> derived description
    PROBE_error_statement_clean = "n=" & Err.Number & ";src=[" & Err.Source & "];desc=" & Err.Description
End Function
