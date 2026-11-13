Attribute VB_Name = "VtProbe"
Option Explicit
' Own worksheet wrapper (MIT, this repository) around the verbatim vba-test classes (TestSuite, TestCase,
' ImmediateReporter) and VBA-Dictionary (the Context of a test case). No FileReporter: it writes files.

Private Function Name(ByVal r As TestResultType) As String
    Select Case r
        Case TestResultType.Pass: Name = "Pass"
        Case TestResultType.Fail: Name = "Fail"
        Case TestResultType.Pending: Name = "Pending"
        Case TestResultType.Skipped: Name = "Skipped"
    End Select
End Function

Public Function VtSuiteCounts(ByVal passes As Long, ByVal fails As Long) As String
    Dim suite As New TestSuite
    Dim i As Long
    suite.Description = "counts"
    For i = 1 To passes
        suite.Test("p" & i).IsEqual i, i
    Next i
    For i = 1 To fails
        suite.Test("f" & i).IsEqual i, i + 1
    Next i
    VtSuiteCounts = Name(suite.Result) & "|" & suite.Tests.Count & "|" & suite.PassedTests.Count & "|" & suite.FailedTests.Count & "|" & suite.PendingTests.Count
End Function

Public Function VtFailureText(ByVal which As Long) As String
    Dim suite As New TestSuite
    Dim t As TestCase
    Set t = suite.Test("t")
    Select Case which
        Case 1: t.IsEqual 1, 2
        Case 2: t.NotEqual "a", "a"
        Case 3: t.IsOk False
        Case 4: t.NotOk 5
        Case 5: t.IsEqual Array(1, 2), Array(1, 3)
        Case 6: t.Includes Array(1, 2, 3), 9
        Case 7: t.IsApproximate 1.2345, 1.3, 3
        Case 8: t.Fail "custom failure"
    End Select
    VtFailureText = Name(t.Result) & "|" & t.Failures.Count & "|" & t.Failures(1)
End Function

Public Function VtPasses(ByVal which As Long) As String
    Dim suite As New TestSuite
    Dim t As TestCase
    Dim c1 As New Collection
    Dim c2 As New Collection
    Set t = suite.Test("t")
    c1.Add 1: c1.Add "x"
    c2.Add 1: c2.Add "x"
    Select Case which
        Case 1: t.IsEqual 1, 1
        Case 2: t.IsEqual 1, 1#
        Case 3: t.IsEqual "abc", "abc"
        Case 4: t.NotEqual 1, 2
        Case 5: t.IsOk True
        Case 6: t.NotOk 0
        Case 7: t.IsEqual Array(1, 2, Array(3, 4)), Array(1, 2, Array(3, 4))
        Case 8: t.IsEqual c1, c2
        Case 9: t.Includes Array(1, 2, 3), 2
        Case 10: t.NotIncludes Array(1, 2, 3), 7
        Case 11: t.IsApproximate 1.2345, 1.2346, 3
        Case 12: t.NotApproximate 1.2345, 1.3, 3
        Case 13: t.IsUndefined Empty
        Case 14: t.Pass
        Case 15: t.Plan 2: t.Pass: t.Pass
    End Select
    VtPasses = Name(t.Result) & "|" & t.Successes & "|" & t.Failures.Count
End Function

Public Function VtPendingSkipped() As String
    Dim suite As New TestSuite
    Dim a As TestCase
    Dim b As TestCase
    Set a = suite.Test("pending")
    Set b = suite.Test("skipped")
    b.Skip
    VtPendingSkipped = Name(a.Result) & "|" & Name(b.Result) & "|" & Name(suite.Result) & "|" & suite.SkippedTests.Count
End Function

Public Function VtGroups() As String
    Dim root As New TestSuite
    Dim g As TestSuite
    Set g = root.Group("inner")
    g.Test("one").IsEqual 1, 1
    g.Test("two").IsEqual 1, 2
    VtGroups = Name(g.Result) & "|" & g.Tests.Count & "|" & g.Parent.Description & "|" & g.Description & "|" & root.Tests.Count
End Function

Public Function VtReporter() As String
    Dim suite As New TestSuite
    Dim rep As New ImmediateReporter
    suite.Description = "reported"
    suite.Test("early").IsEqual 1, 1
    rep.ListenTo suite
    suite.Test("late").IsEqual 1, 2
    VtReporter = Name(suite.Result) & "|" & suite.Tests.Count & "|" & suite.FailedTests.Count
End Function

Public Function VtContext() As String
    Dim suite As New TestSuite
    Dim t As TestCase
    Set t = suite.Test("ctx")
    Set t.Context = New Dictionary
    t.Context.Add "k", 42
    t.IsEqual t.Context("k"), 42
    VtContext = Name(t.Result) & "|" & t.Context.Count
End Function
