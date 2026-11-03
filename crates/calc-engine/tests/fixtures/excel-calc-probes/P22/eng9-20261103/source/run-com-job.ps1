param($OwnedExcel,[string]$Request,[string]$Result)
$ErrorActionPreference='Stop'
$req=Get-Content -LiteralPath $Request -Raw -Encoding utf8|ConvertFrom-Json
$jobResult=$Result;$jobExcel=$OwnedExcel
$repository=(Resolve-Path (Join-Path $PSScriptRoot '../../../../../..')).Path
. (Join-Path $repository 'platform/calc-oracle-excel/worker/capture-worker.ps1')
$Result=$jobResult;$excel=$jobExcel;$directory=Split-Path -Parent $Result
$commonPath=Join-Path $repository 'crates/calc-vba/tests/fixtures/excel-recalculation/calculation-common.bas'
$common=[IO.File]::ReadAllText($commonPath)
$common=$common.Substring(0,$common.IndexOf('Public Function RunCase('))
$module=$common+@'
Private appSink As P22Events
Private changeEdit As Boolean, calcEdit As Boolean, cancelSave As Boolean
Private changedOnce As Boolean, calculatedOnce As Boolean, savedOnce As Boolean
Public Sub Initialize(ByVal ident As String, ByVal changeFlag As Boolean, ByVal calcFlag As Boolean, ByVal saveFlag As Boolean, ByVal cancelFlag As Boolean)
    currentCase = ident
    currentStage = "initialize"
    ReDim trace(0 To 200000)
    used = 0
    loggerReady = True
    Set observedSheet = ThisWorkbook.Worksheets("S")
    changeEdit = changeFlag
    calcEdit = calcFlag
    editOnSave = saveFlag
    cancelSave = cancelFlag
    changedOnce = False
    calculatedOnce = False
    savedOnce = False
    Set appSink = New P22Events
    Set appSink.App = Application
    Profile
End Sub
Public Sub Observe(ByVal kind As String, ByVal detail As String)
    Note kind, detail
    Snap observedSheet, "A1:D1"
End Sub
Public Sub WorksheetChange(ByVal target As Range)
    Observe "Worksheet_Change entry", target.Address(False, False)
    If changeEdit And Not changedOnce Then
        changedOnce = True
        observedSheet.Range("D1").Value2 = observedSheet.Range("A1").Value2 + 100
        Observe "Worksheet_Change edit returned", "D1"
    End If
    Observe "Worksheet_Change exit", target.Address(False, False)
End Sub
Public Sub WorksheetCalculate()
    Observe "Worksheet_Calculate entry", ""
    If calcEdit And Not calculatedOnce Then
        calculatedOnce = True
        observedSheet.Range("A1").Value2 = 7
        Observe "Worksheet_Calculate edit returned", "A1"
    End If
    Observe "Worksheet_Calculate exit", ""
End Sub
Public Sub BeforeSave(ByVal source As String, ByVal saveAsUI As Boolean, ByRef cancel As Boolean)
    Observe source & " BeforeSave entry", "SaveAsUI=" & Typed(saveAsUI) & ";Cancel=" & Typed(cancel)
    If source = "Workbook" Then
        If editOnSave And Not savedOnce Then
            savedOnce = True
            observedSheet.Range("A1").Value2 = 9
            Observe "BeforeSave edit returned", "A1"
        End If
        If cancelSave Then cancel = True
    End If
    Observe source & " BeforeSave exit", "SaveAsUI=" & Typed(saveAsUI) & ";Cancel=" & Typed(cancel)
End Sub
Public Function ReadLog() As String
    Dim lines() As String, i As Long
    If used = 0 Then Exit Function
    ReDim lines(0 To used - 1)
    For i = 0 To used - 1
        lines(i) = trace(i)
    Next
    ReadLog = Join(lines, vbLf)
End Function
Public Sub Shutdown()
    If Not appSink Is Nothing Then Set appSink.App = Nothing
    Set appSink = Nothing
End Sub
'@
$addon=$module.Substring($common.Length)
$firstProcedure=$addon.IndexOf('Public Sub Initialize(')
$declarations=$addon.Substring(0,$firstProcedure)
$module=$common.Replace('Public Sub Note(',$declarations+"`r`nPublic Sub Note(")+$addon.Substring($firstProcedure)
$class=@'
Public WithEvents App As Excel.Application
Private Sub App_SheetChange(ByVal Sh As Object, ByVal Target As Range)
    If Sh.Parent Is ThisWorkbook Then P22Probe.Observe "Application_SheetChange", Sh.Name & "!" & Target.Address(False, False)
End Sub
Private Sub App_SheetCalculate(ByVal Sh As Object)
    If Sh.Parent Is ThisWorkbook Then P22Probe.Observe "Application_SheetCalculate", Sh.Name
End Sub
Private Sub App_WorkbookBeforeSave(ByVal Wb As Workbook, ByVal SaveAsUI As Boolean, Cancel As Boolean)
    If Wb Is ThisWorkbook Then P22Probe.BeforeSave "Application", SaveAsUI, Cancel
End Sub
'@
$sheetCode=@'
Private Sub Worksheet_Change(ByVal Target As Range)
    P22Probe.WorksheetChange Target
End Sub
Private Sub Worksheet_Calculate()
    P22Probe.WorksheetCalculate
End Sub
'@
$bookCode=@'
Private Sub Workbook_SheetCalculate(ByVal Sh As Object)
    P22Probe.Observe "Workbook_SheetCalculate", Sh.Name
End Sub
Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)
    P22Probe.BeforeSave "Workbook", SaveAsUI, Cancel
End Sub
'@
foreach($item in @(@('module.bas',$module),@('events.cls',$class),@('sheet.bas',$sheetCode),@('workbook.bas',$bookCode))){[IO.File]::WriteAllText((Join-Path $directory $item[0]),$item[1],[Text.UTF8Encoding]::new($false))}
$operations=[Collections.Generic.List[object]]::new();$setup=[Collections.Generic.List[object]]::new();$book=$null;$initialized=$false;$fatal=$null;$artifactError=$null;$artifactBefore=$null;$artifactAfter=$null
function Macro([string]$Name,[object[]]$Arguments=@()){$arguments2=@(("'"+$book.Name+"'!P22Probe."+$Name))+$Arguments;Invoke-ComMethod $excel 'Run' $arguments2}
function Snapshot-P22 {
    $cells=@();foreach($a in 'A1','B1','C1','D1'){$r=$book.Worksheets.Item('S').Range($a);$cells+=@{address=$a;value=(Read-Value $r);formula=(Get-ComProperty $r 'Formula2')}}
    return @{cells=$cells;mode=(Get-ComProperty $excel 'Calculation');state=(Get-ComProperty $excel 'CalculationState');events=(Get-ComProperty $excel 'EnableEvents');saved=(Get-ComProperty $book 'Saved')}
}
function Step-P22([string]$Label,[scriptblock]$Action){
    $stageBefore=Snapshot-P22
    Macro 'Stage' @($Label)
    $before=Snapshot-P22;$errorText=$null;$value=$null
    $operations.Add(@{operation=('diagnostic Stage '+$Label);before=$stageBefore;after=$before;utc=[DateTime]::UtcNow.ToString('O')})
    try{$value=& $Action}catch{$errorText=$_.Exception.ToString()}
    $operations.Add(@{operation=$Label;utc=[DateTime]::UtcNow.ToString('O');before=$before;after=(Snapshot-P22);returned=$value;error=$errorText})
}
function Set-Code($Component,[string]$Code){
    $count=$Component.CodeModule.CountOfLines
    if($count -gt 0){$Component.CodeModule.DeleteLines(1,$count)}
    $Component.CodeModule.AddFromString($Code)
}
$securityBefore=Get-ComProperty $excel 'AutomationSecurity'
try{
    if($securityBefore -ne 1){throw 'P22 needs the existing AutomationSecurity=1 context; no policy is changed.'}
    $excel.EnableEvents=$false
    $book=$excel.Workbooks.Add(-4167);$sheet=$book.Worksheets.Item(1);$sheet.Name='S'
    $excel.Calculation=-4135;$excel.MultiThreadedCalculation.Enabled=$false;$excel.Iteration=$false;$excel.CalculateBeforeSave=$false
    $setup.Add(@{operation='manual blank workbook';snapshot=(Snapshot-P22)})
    $sheet.Range('A1').Value2=1.0;$setup.Add(@{operation='assign A1=1';snapshot=(Snapshot-P22)})
    $sheet.Range('B1').Formula2='=A1+1';$setup.Add(@{operation='assign B1 formula';snapshot=(Snapshot-P22)})
    $sheet.Range('C1').Formula2='=B1*2';$setup.Add(@{operation='assign C1 formula';snapshot=(Snapshot-P22)})
    $excel.CalculateFull()
    $setup.Add(@{operation='setup CalculateFull';snapshot=(Snapshot-P22)})
    $project=$book.VBProject
    $component=$project.VBComponents.Add(1);$component.Name='P22Probe';Set-Code $component $module
    $component=$project.VBComponents.Add(2);$component.Name='P22Events';Set-Code $component $class
    Set-Code ($project.VBComponents.Item($sheet.CodeName)) $sheetCode
    Set-Code ($project.VBComponents.Item($book.CodeName)) $bookCode
    foreach($name in @('P22Probe','P22Events',$sheet.CodeName,$book.CodeName)){$code=$project.VBComponents.Item($name).CodeModule;[IO.File]::WriteAllText((Join-Path $directory ('loaded-'+$name+'.bas')),[string]$code.Lines(1,$code.CountOfLines),[Text.UTF8Encoding]::new($false))}
    Macro 'Initialize' @([string]$req.case,[bool]$req.change_edit,[bool]$req.calc_edit,[bool]$req.save_edit,[bool]$req.cancel_save)
    $initialized=$true
    Step-P22 'enable-events' {$excel.EnableEvents=[bool]$req.events}
    Step-P22 'set-calculation-mode' {$excel.Calculation=$(if($req.mode -eq 'automatic'){-4105}else{-4135})}
    foreach($operation in $req.operations){switch($operation){
        'write' {Step-P22 'COM write A1=5' {$sheet.Range('A1').Value2=5.0}}
        'same' {Step-P22 'COM same-value rewrite' {$sheet.Range('A1').Value2=[double]$sheet.Range('A1').Value2}}
        'clear' {Step-P22 'COM ClearContents A1' {$sheet.Range('A1').ClearContents()}}
        'calculate' {Step-P22 'COM Application.Calculate' {$excel.Calculate()}}
        'full' {Step-P22 'COM Application.CalculateFull' {$excel.CalculateFull()}}
        'rebuild' {Step-P22 'COM Application.CalculateFullRebuild' {$excel.CalculateFullRebuild()}}
        'sheet' {Step-P22 'COM Worksheet.Calculate' {$sheet.Calculate()}}
        'range' {Step-P22 'COM Range.Calculate B1' {$sheet.Range('B1').Calculate()}}
        'saveas' {Step-P22 'COM SaveAs' {$book.SaveAs((Join-Path $directory 'tested.xlsm'),52)}}
        'save' {Step-P22 'COM Save' {$book.Save()}}
        'copy' {Step-P22 'COM SaveCopyAs' {$book.SaveCopyAs((Join-Path $directory 'tested-copy.xlsm'))}}
        'undo' {Step-P22 'COM Application.Undo' {$excel.Undo()}}
        default {throw "Unknown P22 operation $operation"}
    }}
    Step-P22 'disable-events diagnostic boundary' {$excel.EnableEvents=$false}
    $trace=[string](Macro 'ReadLog')
    [IO.File]::WriteAllText((Join-Path $directory 'trace.tsv'),$trace,[Text.UTF8Encoding]::new($false))
}catch{$fatal=$_.Exception.ToString()}
finally{
    $excel.EnableEvents=$false
    if($initialized){try{Macro 'Shutdown'}catch{if(-not $fatal){$fatal=$_.Exception.ToString()}}}
    if($book){try{$artifactBefore=Snapshot-P22;$book.SaveAs((Join-Path $directory 'artifact.xlsm'),52);$artifactAfter=Snapshot-P22}catch{$artifactError=$_.Exception.ToString()};$book.Close($false)}
}
$securityAfter=Get-ComProperty $excel 'AutomationSecurity'
if($securityAfter -ne $securityBefore){$fatal='AutomationSecurity changed unexpectedly; observation refused.'}
$files=[ordered]@{};Get-ChildItem -LiteralPath $directory -File|Where-Object Name -ne 'result.json'|ForEach-Object{$files[$_.Name]=(Get-FileHash $_.FullName).Hash.ToLowerInvariant()}
$output=@{fatal=$fatal;case=$req.case;trial=$req.trial;setup=$setup;operations=$operations;artifact_save=@{before=$artifactBefore;after=$artifactAfter;error=$artifactError};automation_security_before=$securityBefore;automation_security_after=$securityAfter;
    excel_version=[string]$excel.Version;excel_build=[string]$excel.Build;locale=(Get-Culture).Name;common_source_sha256=(Get-FileHash $commonPath).Hash.ToLowerInvariant();files_sha256=$files;
    limitations='Ordinary COM commands with in-process VBA event logging. Macro Stage calls and diagnostic cache reads are retained; UI, paste/Redo, two-sheet, async and RTD conditions are not inferred. Artifact save follows the tested operations with events disabled.'}
[IO.File]::WriteAllText($Result,((ConvertTo-Json -InputObject $output -Depth 40)+"`n"),[Text.UTF8Encoding]::new($false))
