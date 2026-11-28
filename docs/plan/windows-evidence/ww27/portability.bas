
Option Explicit
Private Declare PtrSafe Function MissingLib Lib "C:\DnaWw27_FFC03F35_absent.dll" () As Long
Private Declare PtrSafe Function MissingEntry Lib "kernel32.dll" Alias "DnaWw27_FFC03F35_AbsentExport" () As Long
Private Declare PtrSafe Function UnusedLib Lib "C:\DnaWw27_FFC03F35_unused.dll" () As Long
Private rows As String
Private scratch As String
Private Sub Record(ByVal id As String, ByVal inputText As String, ByVal v As Variant, ByVal no As Long, ByVal src As String, ByVal desc As String, ByVal lineNo As Long)
    Dim valueText As String
    If IsObject(v) Then
        valueText = TypeName(v)
    ElseIf Not IsEmpty(v) Then
        valueText = CStr(v)
    End If
    rows = rows & id & vbTab & inputText & vbTab & TypeName(v) & vbTab & valueText & vbTab & no & vbTab & src & vbTab & desc & vbTab & lineNo & vbLf
End Sub
Public Function Unrelated() As Long
    Unrelated = 2701
End Function
Public Function Capture(ByVal scratchPath As String, ByVal missingDrive As String, ByVal missingShare As String) As String
    Dim f As Integer, item As Variant, i As Long, marker As String, p As String, flags As Variant, val As Variant
    Dim initial As String
    initial = CurDir$
    scratch = scratchPath
    rows = "case" & vbTab & "input" & vbTab & "type" & vbTab & "value" & vbTab & "err" & vbTab & "source" & vbTab & "description" & vbTab & "erl" & vbLf
    #If Win64 Then
        Record "ExcelBits", "", 64, 0, "", "", 0
    #Else
        Record "ExcelBits", "", 32, 0, "", "", 0
    #End If
    f = FreeFile
    Open scratch & "\from.txt" For Output As #f
    Print #f, "source"
    Close #f
    ProbeCurDir "DriveCurDir", missingDrive
    ProbeChDrive "DriveChDrive", missingDrive
    ProbeChDir "DriveChDir", missingDrive & ":\"
    ProbeOpenInput "DriveOpenInput", missingDrive & ":\ww27-absent\item.txt"
    ProbeOpenOutput "DriveOpenOutput", missingDrive & ":\ww27-absent\item.txt"
    ProbeDir "DriveDir", missingDrive & ":\ww27-absent\item.txt"
    ProbeKill "DriveKill", missingDrive & ":\ww27-absent\item.txt"
    ProbeMoveFrom "DriveMoveFrom", missingDrive & ":\ww27-absent\item.txt"
    ProbeMoveTo "DriveMoveTo", missingDrive & ":\ww27-absent\item.txt"
    ProbeCopyFrom "DriveCopyFrom", missingDrive & ":\ww27-absent\item.txt"
    ProbeCopyTo "DriveCopyTo", missingDrive & ":\ww27-absent\item.txt"
    ProbeMkDir "DriveMkDir", missingDrive & ":\ww27-absent\item.txt"
    ProbeGetAttr "DriveGetAttr", missingDrive & ":\ww27-absent\item.txt"
    ProbeFileDateTime "DriveFileDateTime", missingDrive & ":\ww27-absent\item.txt"
    ProbeFileLen "DriveFileLen", missingDrive & ":\ww27-absent\item.txt"
    ProbeCurDir "UNCCurDir", missingShare
    ProbeChDrive "UNCChDrive", missingShare
    ProbeChDir "UNCChDir", missingShare
    ProbeOpenInput "UNCOpenInput", missingShare & "\item.txt"
    ProbeOpenOutput "UNCOpenOutput", missingShare & "\item.txt"
    ProbeDir "UNCDir", missingShare & "\item.txt"
    ProbeKill "UNCKill", missingShare & "\item.txt"
    ProbeMoveFrom "UNCMoveFrom", missingShare & "\item.txt"
    ProbeMoveTo "UNCMoveTo", missingShare & "\item.txt"
    ProbeCopyFrom "UNCCopyFrom", missingShare & "\item.txt"
    ProbeCopyTo "UNCCopyTo", missingShare & "\item.txt"
    ProbeMkDir "UNCMkDir", missingShare & "\item.txt"
    ProbeGetAttr "UNCGetAttr", missingShare & "\item.txt"
    ProbeFileDateTime "UNCFileDateTime", missingShare & "\item.txt"
    ProbeFileLen "UNCFileLen", missingShare & "\item.txt"
    AttributesAndDir
    MissingHosts False
    MissingHosts True
    LateClass
    EnvironmentCases
    PortableObjects
    ViewVerbs
    AppMembers
    FileContext
    ChDir initial
    Capture = rows
End Function
Private Sub AttributesAndDir()
    Dim f As Integer, p As String, item As Variant, flags As Variant
    For Each item In Array("zeta.txt", "Alpha.txt", "beta.TXT", "NoExt", "a-b.txt", "a_b.txt", "case.txt")
        f = FreeFile
        Open scratch & "\" & CStr(item) For Output As #f
        Close #f
        Record "NewFileGetAttr", CStr(item), GetAttr(scratch & "\" & CStr(item)), 0, "", "", 0
    Next
    SetAttr scratch & "\zeta.txt", vbHidden + vbArchive
    MkDir scratch & "\sub"
    Dim mask As Variant, result As String
    For Each mask In Array("*", "*.*")
        For Each flags In Array(0, vbHidden, vbDirectory, vbHidden + vbDirectory + vbArchive)
            result = Dir$(scratch & "\" & mask, flags)
            Do While Len(result) > 0
                Record "DirOrder", CStr(mask) & "/" & flags, result, 0, "", "", 0
                result = Dir$
            Loop
        Next
    Next
    On Error Resume Next
    For Each item In Array("case.txt.", "case.txt ", "case.txt. ")
        p = scratch & "\" & CStr(item)
        Err.Clear
140     result = Dir$(p)
        Record "TrailingDir", p, result, Err.Number, Err.Source, Err.Description, Erl
        Err.Clear
150     result = CStr(GetAttr(p))
        Record "TrailingGetAttr", p, result, Err.Number, Err.Source, Err.Description, Erl
    Next
    Err.Clear
160 Name scratch & "\case.txt" As scratch & "\CASE.txt"
    Record "CaseOnlyName", "case.txt -> CASE.txt", Empty, Err.Number, Err.Source, Err.Description, Erl
    Err.Clear
    Record "CaseOnlyDir", "case.*", Dir$(scratch & "\case.*"), Err.Number, Err.Source, Err.Description, Erl
End Sub
Private Sub MissingHosts(ByVal takeBranch As Boolean)
    Dim v As Variant
    On Error Resume Next
    Err.Clear
    If takeBranch Then
200     v = MissingLib()
    End If
    Record "DeclareMissingDll", CStr(takeBranch), v, Err.Number, Err.Source, Err.Description, Erl
    v = Empty: Err.Clear
    If takeBranch Then
210     v = MissingEntry()
    End If
    Record "DeclareMissingEntry", CStr(takeBranch), v, Err.Number, Err.Source, Err.Description, Erl
    v = Empty: Err.Clear
    If takeBranch Then
220     v = Shell(scratch & "\absent-FFC03F35.exe", vbHide)
    End If
    Record "ShellMissing", CStr(takeBranch), v, Err.Number, Err.Source, Err.Description, Erl
    Err.Clear
    Record "Unrelated", CStr(takeBranch), Unrelated(), Err.Number, Err.Source, Err.Description, Erl
End Sub
Private Sub LateClass()
    Dim o As Object, id As String
    id = "DnaWw27FFC03F35.AbsentClass"
    On Error Resume Next
    Err.Clear
230 Set o = CreateObject(id)
    Record "CreateObject", id, o, Err.Number, Err.Source, Err.Description, Erl
    Err.Clear
240 Set o = GetObject("", id)
    Record "GetObjectEmpty", id, o, Err.Number, Err.Source, Err.Description, Erl
    Err.Clear
250 Set o = GetObject(, id)
    Record "GetObjectOmitted", id, o, Err.Number, Err.Source, Err.Description, Erl
    Err.Clear
260 Set o = GetObject(scratch & "\absent-file.zzz", id)
    Record "GetObjectMissingFile", id, o, Err.Number, Err.Source, Err.Description, Erl
End Sub
Private Sub EnvironmentCases()
    Dim i As Long, text As String, k As Variant, v As Variant
    On Error Resume Next
    For i = 1 To 255
        text = Environ$(i)
        If Len(text) = 0 Then Exit For
        If UCase$(Left$(text, 5)) = "WW27_" Then Record "EnvironmentOrder", CStr(i), text, 0, "", "", 0
    Next
    For Each k In Array("WW27_Z_MARK", "WW27_A_MARK", "ww27_mixed", "WW27_" & ChrW(201) & "_MARK", "WW27_DOES_NOT_EXIST_FFC03F35", "TEMP", "TMP", "USERPROFILE", "APPDATA", 40000, 0, 256)
        v = Empty: Err.Clear
270     v = Environ$(k)
        If k = "TEMP" Or k = "TMP" Or k = "USERPROFILE" Or k = "APPDATA" Then
            If Len(v) > 0 Then v = Left$(CStr(v), 3) & "<redacted>; length=" & Len(v)
        End If
        Record "Environ", CStr(k), v, Err.Number, Err.Source, Err.Description, Erl
    Next
End Sub
Private Sub ViewVerbs()
    Dim ws As Worksheet, v As Variant, verb As String
    Set ws = ThisWorkbook.Worksheets(1)
    ws.Name = "Loan"
    ws.Activate
    On Error Resume Next
    Err.Clear
300 v = Application.Run("DnaCalc.View.Switch", "notebook", "Summary")
    Record "ViewSwitch", "notebook/Summary", v, Err.Number, Err.Source, Err.Description, Erl
    v = Empty: Err.Clear
310 v = Application.Run("DnaCalc.View.Select", "Loan!B2:B6")
    Record "ViewSelect", "Loan!B2:B6", v, Err.Number, Err.Source, Err.Description, Erl
    v = Empty: Err.Clear
320 v = Application.Run("DnaCalc.View.ShowPanel", "inspector.trace", True)
    Record "ViewShowPanel", "inspector.trace/True", v, Err.Number, Err.Source, Err.Description, Erl
    Err.Clear
330 Worksheets("Loan").Range("B2:B6").Select
    Record "ExcelSelection", "Loan!B2:B6", Selection.Address, Err.Number, Err.Source, Err.Description, Erl
    Err.Clear
340 ws.Range("C2").Value2 = 27
    Record "ExcelWrite", "Loan!C2", ws.Range("C2").Value2, Err.Number, Err.Source, Err.Description, Erl
End Sub
Private Sub AppMembers()
    Dim v As Variant, o As Object, p As String
    p = scratch & "\missing-FFC03F35.xll"
    On Error Resume Next
    Err.Clear
350 v = Application.RegisterXLL(p)
    Record "RegisterXLL", p, v, Err.Number, Err.Source, Err.Description, Erl
    Err.Clear
360 Set o = Application.FileDialog(3)
    Record "FileDialog", "3", o, Err.Number, Err.Source, Err.Description, Erl
    v = Empty: Err.Clear
370 v = o.SelectedItems.Count
    Record "SelectedItemsCount", "no selection", v, Err.Number, Err.Source, Err.Description, Erl
    v = Empty: Err.Clear
380 v = o.SelectedItems(1)
    Record "SelectedItemsOne", "no selection", v, Err.Number, Err.Source, Err.Description, Erl
    Set o = Nothing: Err.Clear
390 Set o = Application.Dialogs(xlDialogOpen)
    Record "Dialogs", "xlDialogOpen", o, Err.Number, Err.Source, Err.Description, Erl
    v = Empty: Err.Clear
400 v = Application.Wait(Now)
    Record "Wait", "Now", v, Err.Number, Err.Source, Err.Description, Erl
End Sub
Private Sub FileContext()
    Dim v As Variant, initial As String, saved As String
    initial = CurDir$
    PathSnapshot "unsaved"
    saved = scratch & "\file-context.xlsx"
    Application.DisplayAlerts = False
    Application.DisplayAlerts = True
End Sub
Private Sub PathSnapshot(ByVal stage As String)
    Record "WorkbookPath", stage, ThisWorkbook.Path, 0, "", "", 0
    Record "WorkbookFullName", stage, ThisWorkbook.FullName, 0, "", "", 0
    Record "DefaultFilePath", stage, Application.DefaultFilePath, 0, "", "", 0
    Record "PathSeparator", stage, Application.PathSeparator, 0, "", "", 0
    Record "CurDir", stage, CurDir$, 0, "", "", 0
    Record "CurDirC", stage, CurDir$("C"), 0, "", "", 0
    Record "CellFilenameOmitted", stage, Application.Evaluate("CELL(""filename"")"), 0, "", "", 0
    Record "CellFilenameExplicit", stage, Application.Evaluate("CELL(""filename"",Loan!C2)"), 0, "", "", 0
    Record "InfoDirectory", stage, Application.Evaluate("INFO(""directory"")"), 0, "", "", 0
End Sub

Private Sub ProbeCurDir(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 v = CurDir$(p)
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeChDrive(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 ChDrive p
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeChDir(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 ChDir p
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeOpenInput(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 Open p For Input As #f: Close #f
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeOpenOutput(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 Open p For Output As #f: Close #f
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeDir(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 v = Dir$(p)
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeKill(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 Kill p
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeMoveFrom(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 Name p As scratch & "\moved.txt"
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeMoveTo(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 Name scratch & "\from.txt" As p
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeCopyFrom(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 FileCopy p, scratch & "\copied.txt"
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeCopyTo(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 FileCopy scratch & "\from.txt", p
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeMkDir(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 MkDir p
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeGetAttr(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 v = GetAttr(p)
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeFileDateTime(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 v = FileDateTime(p)
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub


Private Sub ProbeFileLen(ByVal id As String, ByVal p As String)
    Dim f As Integer, v As Variant
    On Error Resume Next
    f = FreeFile
    Err.Clear
100 v = FileLen(p)
    Record id, p, v, Err.Number, Err.Source, Err.Description, Erl
End Sub

Private Sub PortableObjects()
    Dim fso As Object, stm As Object, doc As Object, shell As Object, streamFile As String, item As Variant, o As Object
    Dim v As Variant, names As Variant, i As Integer, hexText As String, prefix As String
    On Error Resume Next
    Err.Clear
500 Set fso = CreateObject("Scripting.FileSystemObject")
    Record "CreateFSO", "", fso, Err.Number, Err.Source, Err.Description, Erl
    If Not fso Is Nothing Then
        For Each item In Array("C:\scratch\missing.TxT", "C:", "C:drive.Relative", "C:\a.b\NoExt", "C:\a.b\", "\\localhost\share\", ".hidden", "UPPER.CAF" & ChrW(201))
            v = Empty: Err.Clear
510         v = fso.GetFileName(Path:=CStr(item))
            Record "FSOGetFileNameNamed", CStr(item), v, Err.Number, Err.Source, Err.Description, Erl
            v = Empty: Err.Clear
520         v = fso.GetFileName(CStr(item))
            Record "FSOGetFileName", CStr(item), v, Err.Number, Err.Source, Err.Description, Erl
            v = Empty: Err.Clear
530         v = fso.GetExtensionName(CStr(item))
            Record "FSOGetExtensionName", CStr(item), v, Err.Number, Err.Source, Err.Description, Erl
        Next
        Err.Clear
540     Set o = fso.CreateTextFile(scratch & "\FsoNew.txt", False, False)
        Record "FSOCreateTextFile", "FsoNew.txt", o, Err.Number, Err.Source, Err.Description, Erl
        If Not o Is Nothing Then o.Write "ww27": o.Close
        v = Empty: Err.Clear
550     v = fso.FileExists(scratch & "\fsonew.TXT")
        Record "FSOFileExists", "fsonew.TXT", v, Err.Number, Err.Source, Err.Description, Erl
        Set o = Nothing: Err.Clear
560     Set o = fso.GetFile(scratch & "\absent.txt")
        Record "FSOGetFileAbsent", "absent.txt", o, Err.Number, Err.Source, Err.Description, Erl
    End If
    Err.Clear
570 Set stm = CreateObject("ADODB.Stream")
    Record "CreateStream", "", stm, Err.Number, Err.Source, Err.Description, Erl
    If Not stm Is Nothing Then
        v = Empty: Err.Clear
580     v = stm.Size
        Record "StreamClosedSize", "", v, Err.Number, Err.Source, Err.Description, Erl
        stm.Open
        Record "StreamDefaultType", "", stm.Type, 0, "", "", 0
        Record "StreamDefaultCharset", "", stm.Charset, 0, "", "", 0
        stm.Type = 2: stm.Charset = "UTF-8"
        stm.WriteText "caf" & ChrW(233)
        Record "StreamPosition", "UTF-8 café", stm.Position, 0, "", "", 0
        Record "StreamSize", "UTF-8 café", stm.Size, 0, "", "", 0
        Err.Clear
590     stm.Type = 1
        Record "StreamChangeTypeNonzeroPosition", "binary", Empty, Err.Number, Err.Source, Err.Description, Erl
        stm.Position = 0
        Record "StreamReadText", "UTF-8 café", stm.ReadText, 0, "", "", 0
        stm.Position = 0: stm.Type = 1
        v = stm.Read
        For i = LBound(v) To UBound(v)
            hexText = hexText & Right$("0" & Hex$(v(i)), 2)
        Next
        Record "StreamReadBinary", "UTF-8 café", hexText, 0, "", "", 0
        v = Empty: Err.Clear
600     v = stm.ReadText
        Record "StreamTextOnBinary", "", v, Err.Number, Err.Source, Err.Description, Erl
        streamFile = scratch & "\stream.bin"
        stm.SaveToFile streamFile, 2
        Err.Clear
610     stm.SaveToFile streamFile, 1
        Record "StreamNoOverwrite", "stream.bin/1", Empty, Err.Number, Err.Source, Err.Description, Erl
        Err.Clear
620     stm.SaveToFile streamFile, 2
        Record "StreamOverwrite", "stream.bin/2", Empty, Err.Number, Err.Source, Err.Description, Erl
        stm.Close
    End If
    Err.Clear
630 Set doc = CreateObject("MSXML2.DOMDocument.6.0")
    Record "CreateXML", "", doc, Err.Number, Err.Source, Err.Description, Erl
    If Not doc Is Nothing Then
        doc.resolveExternals = False: doc.validateOnParse = False
        Record "XmlMalformed", "<r><x></r>", doc.LoadXML("<r><x></r>"), 0, "", "", 0
        Record "XmlErrorCode", "", doc.parseError.errorCode, 0, "", "", 0
        Record "XmlErrorReason", "", Trim$(Replace(Replace(doc.parseError.reason, vbCr, ""), vbLf, "")), 0, "", "", 0
        Record "XmlErrorLine", "", doc.parseError.Line, 0, "", "", 0
        Record "XmlErrorLinepos", "", doc.parseError.linepos, 0, "", "", 0
        doc.LoadXML "<r><x>first</x><x>second</x></r>"
        Set o = CreateObject("MSXML2.DOMDocument.6.0")
        o.resolveExternals = False: o.validateOnParse = False
        o.LoadXML "<r><x>independent</x></r>"
        Record "XmlXPath", "/r/x[2]", doc.selectSingleNode("/r/x[2]").Text, 0, "", "", 0
        Record "XmlIndependent", "/r/x", o.selectSingleNode("/r/x").Text, 0, "", "", 0
    End If
    Err.Clear
640 Set shell = CreateObject("WScript.Shell")
    Record "CreateWsh", "", shell, Err.Number, Err.Source, Err.Description, Erl
    If Not shell Is Nothing Then
        Record "WshSpecialFoldersCount", "", shell.SpecialFolders.Count, 0, "", "", 0
        Record "WshExpand", "%WW27_A_MARK% / %WW27_DOES_NOT_EXIST_FFC03F35%", shell.ExpandEnvironmentStrings("%WW27_A_MARK% / %WW27_DOES_NOT_EXIST_FFC03F35%"), 0, "", "", 0
        prefix = shell.CurrentDirectory
        shell.CurrentDirectory = scratch
        Record "WshCurrentDirectory", scratch, shell.CurrentDirectory, 0, "", "", 0
        shell.CurrentDirectory = prefix
    End If
End Sub
