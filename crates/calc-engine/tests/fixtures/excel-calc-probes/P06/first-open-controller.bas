Private Function CaseIds() As String
    CaseIds = "first-open-controller"
End Function
Private Sub ProbeCase(ByVal ident As String, ByVal output As String)
    Note "first-open-controller", "The first workbook was observed and closed before this controlled bootstrap was opened and compiled. See first-open-observation.json."
End Sub
