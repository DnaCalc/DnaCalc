param([ValidateSet('build','cases','reopen','activeobjects','project','inbook','language')][string]$Action, [string]$OutDirectory, [string]$AddIn,
    [string]$Cases, [string]$Project, [string[]]$Files, [string]$FormulaCell = 'H1', [bool]$InBook = $false, $OwnedExcel = $null)
# The Excel side of OneCalc milestone 5 (dc-0f1k.4), in an owned hidden Excel session
# (tools/excel-session). Run without -OwnedExcel, this script submits itself as one job; run it
# through tools/excel-session/on-private-desktop.ps1 so no Excel window reaches the desktop.
#   build:  a new workbook gets DnaExamples.bas, DnaSimple.bas, DnaCounter.cls, ThisWorkbook.txt and the
#           Microsoft Scripting Runtime reference through the VBE object model, and is saved as
#           DnaExamples.xlsm and DnaExamples.xlam in -OutDirectory; DnaSimple.bas alone is saved as
#           DnaSimple.xlsm.
#   cases:  opens the .xlam (-AddIn) as Excel opens an add-in file, enters each case formula of
#           -Cases in -FormulaCell of a new workbook and records Value2, Text and its type; with
#           -InBook, -AddIn is a macro workbook and the formulas go into its own first sheet.
#   activeobjects: a new workbook with two sheets and a module of UDFs that read ActiveSheet,
#           Selection, ActiveWorkbook and Application.Caller; their answers are recorded while the
#           user's active sheet and selection differ from the calling cell's sheet and address
#           (dc-0f1k.12).
#   project: the VBA validation corpus (docs/plan/VBA_VALIDATION.md). -Project is a corpus
#           project.json (crates/calc-vba/tests/corpus/<id>/): a new workbook gets the project's modules
#           (standard and class, from source files) and listed references through the VBE object model,
#           each formula of the project's cases file is entered in -FormulaCell and Value2 recorded.
#           Nothing is opened from outside: the workbook is ours and fresh; third-party source is only
#           imported, and only the functions its cases name are called. OutDirectory gets cases.json (the
#           cases file with Excel's answers, no prediction) and project.json rows with types and text.
#   inbook: like project, but the formulas stay in the workbook: formula i of the cases file goes into row 1,
#           column 1+8*(i-1) of Sheet1 (a spill of up to 7 columns stays clear of the next case), the workbook is
#           calculated and saved as <OutDirectory>/<project id>.xlsm, and inbook-rows.json records per cell the
#           Value2 (with its type), Text and NumberFormat (Excel keeps no Date or Currency type in a cell; the VBA
#           return type is read from the declaration by the corpus tooling).
#           The capture worker then records Excel's typed answers of that file (dnacalc-capture-record).
#   language: CalcVBA's basic-language conformance rows (conformance/excel-measurements/gen_language_rows.py
#           prepares them). -Project is the manifest of one batch: each item's module is imported next to
#           the recording helper into a new workbook, the helper's DnaRunAll runs Main, and the records it
#           returns (or the VBA error it caught) are stored per item in language.json. A dialog the
#           owned Excel raises (compile error, run-time error) is captured by a UI Automation guard
#           that reads its text and presses End or OK, never a mouse or key.
#   reopen: opens each saved .xlsm of -Files, reads its formula and calculated value, and records
#           whether Excel repaired it (the workbook name and the error*.xml repair logs in TEMP).
# No Excel setting, Trust Center or macro-security value is read for change or written; the
# session host checks AutomationSecurity before and after. A failed step is recorded, not retried.
$ErrorActionPreference = 'Stop'
if ($null -eq $OwnedExcel) {
    $parameters = @{}
    foreach ($key in $PSBoundParameters.Keys) { $parameters[$key] = $PSBoundParameters[$key] }
    $client = Join-Path $PSScriptRoot '../../../../tools/excel-session/submit.ps1'
    & $client -Script $PSCommandPath -Parameters ($parameters | ConvertTo-Json -Depth 6 -Compress) -Environment 'vba:dnaexamples' -TimeoutSeconds 600
    return
}
$excel = $OwnedExcel
# -Files may arrive as one comma-separated text (a caller's -File argument).
$Files = @($Files | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
New-Item -ItemType Directory -Force -Path $OutDirectory | Out-Null
$result = [ordered]@{ action = $Action; excel_version = [string]$excel.Version; excel_build = [string]$excel.Build; automation_security = $excel.AutomationSecurity; rows = @(); errors = @() }
function Step($name, [scriptblock]$body) { try { & $body } catch { $script:result.errors += ("$name`: " + $_.Exception.Message) } }
function Describe($value) {
    if ($null -eq $value) { return @{ blank = $true } }
    if ($value -is [int] -and $value -le -2146826200 -and $value -ge -2146826300) { return @{ error_code = $value } }
    return @{ value = $value; type = $value.GetType().Name }
}
function Start-DialogGuard([uint32]$TargetPid) {
    $state = [hashtable]::Synchronized(@{ stop = $false; polls = 0; dialogs = [System.Collections.ArrayList]::Synchronized((New-Object System.Collections.ArrayList)); errors = [System.Collections.ArrayList]::Synchronized((New-Object System.Collections.ArrayList)) })
    $shell = [powershell]::Create()
    [void]$shell.AddScript({
        param($state, $targetPid)
        Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
        # A VBE dialog is not among the UI Automation root's children (it is owned by the VBE window and
        # has no Invoke pattern), so it is found by its window class and pressed through the button's
        # accessible object (the legacy bridge of UI Automation): no mouse, no key, no focus.
        Add-Type @"
using System;using System.Collections.Generic;using System.Runtime.InteropServices;using System.Text;
public static class DlgFinder {
 public delegate bool P(IntPtr h,IntPtr s);
 [DllImport("user32.dll")] static extern bool EnumWindows(P cb,IntPtr s);
 [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr d,P cb,IntPtr s);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
 [DllImport("oleacc.dll")] static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint objectId, ref Guid riid, [MarshalAs(UnmanagedType.Interface)] out object accessible);
 public static object Accessible(long hwnd){Guid iid=new Guid("618736E0-3C3D-11CF-810C-00AA00389B71");object acc;int hr=AccessibleObjectFromWindow(new IntPtr(hwnd),0xFFFFFFFC,ref iid,out acc);if(hr!=0)throw new Exception("AccessibleObjectFromWindow "+hr);return acc;}
 // The Excel Window object of the Excel process with this id, through its EXCEL7 window (never a moniker or CoCreate).
 public static object ExcelWindow(uint pid){object found=null;
  EnumWindows((h,s)=>{uint p;GetWindowThreadProcessId(h,out p);if(p!=pid)return true;var c=new StringBuilder(64);GetClassName(h,c,64);if(c.ToString()!="XLMAIN")return true;
   EnumChildWindows(h,(k,x)=>{var a=new StringBuilder(64);GetClassName(k,a,64);if(a.ToString()=="EXCEL7"&&found==null){Guid iid=new Guid("00020400-0000-0000-C000-000000000046");object o;if(AccessibleObjectFromWindow(k,0xFFFFFFF0,ref iid,out o)==0)found=o;}return true;},IntPtr.Zero);
   return found==null;},IntPtr.Zero);return found;}
 public class Dialog { public long Hwnd; public string Title; public List<string> Texts=new List<string>(); public List<KeyValuePair<string,long>> Buttons=new List<KeyValuePair<string,long>>(); }
 public static List<Dialog> Find(uint pid){var o=new List<Dialog>();
  EnumWindows((h,s)=>{uint p;GetWindowThreadProcessId(h,out p);if(p!=pid||!IsWindowVisible(h))return true;var c=new StringBuilder(128);GetClassName(h,c,128);if(c.ToString()!="#32770")return true;
   var d=new Dialog();d.Hwnd=h.ToInt64();var t=new StringBuilder(300);GetWindowText(h,t,300);d.Title=t.ToString();
   EnumChildWindows(h,(k,x)=>{var a=new StringBuilder(64);var b=new StringBuilder(2000);GetClassName(k,a,64);GetWindowText(k,b,2000);var cls=a.ToString();
    if(cls=="Button")d.Buttons.Add(new KeyValuePair<string,long>(b.ToString(),k.ToInt64())); else if(b.Length>0)d.Texts.Add(b.ToString());return true;},IntPtr.Zero);
   o.Add(d);return true;},IntPtr.Zero);return o;}
}
"@
        $auto = [System.Windows.Automation.AutomationElement]
        while (-not $state.stop) {
            try {
                $state.polls++
                foreach ($dialog in [DlgFinder]::Find([uint32]$targetPid)) {
                    $names = @($dialog.Buttons | ForEach-Object { $_.Key })
                    $pick = $null
                    foreach ($want in 'End', 'OK', 'Close', 'No', 'Cancel') {
                        $pick = $dialog.Buttons | Where-Object { $_.Key -eq $want } | Select-Object -First 1
                        if ($pick) { break }
                    }
                    $record = [ordered]@{ title = $dialog.Title; texts = @($dialog.Texts); buttons = $names; pressed = $null }
                    if ($pick) {
                        try {
                            try { $auto::FromHandle([IntPtr]$pick.Value).GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke() }
                            catch { $acc = [DlgFinder]::Accessible($pick.Value); [void]$acc.GetType().InvokeMember('accDoDefaultAction', 'InvokeMethod', $null, $acc, @(0)) }
                            $record.pressed = $pick.Key
                        } catch { $record.press_error = $_.Exception.Message }
                        # A compile error leaves the VBE in break mode and the caller's Run waiting:
                        # Run > Reset (command 228) of this process's own VBE ends it.
                        if (($dialog.Texts -join ' ') -match 'Compile error') {
                            try {
                                Start-Sleep -Milliseconds 300
                                $app = [DlgFinder]::ExcelWindow([uint32]$targetPid).Application
                                $check = [uint32]0
                                Add-Type -Namespace Dna -Name Win3 -ErrorAction SilentlyContinue -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr h, out uint p);'
                                [void][Dna.Win3]::GetWindowThreadProcessId([System.IntPtr][int]$app.Hwnd, [ref]$check)
                                if ($check -eq $targetPid) { $app.VBE.CommandBars.FindControl(1, 228).Execute(); $record.reset = $true } else { $record.reset = 'pid mismatch' }
                            } catch { $record.reset_error = $_.Exception.Message }
                        }
                    }
                    [void]$state.dialogs.Add($record)
                }
            } catch { if ($state.errors.Count -lt 20) { [void]$state.errors.Add($_.Exception.Message) } }
            Start-Sleep -Milliseconds 120
        }
    }).AddArgument($state).AddArgument($TargetPid)
    [pscustomobject]@{ State = $state; Shell = $shell; Handle = $shell.BeginInvoke() }
}
function Stop-DialogGuard($guard) {
    $guard.State.stop = $true
    [void]$guard.Handle.AsyncWaitHandle.WaitOne(5000)
    $report = [ordered]@{ polls = $guard.State.polls; errors = @($guard.State.errors) + @($guard.Shell.Streams.Error | ForEach-Object { [string]$_ }) }
    $guard.Shell.Dispose()
    $report
}
function Excel-ProcessId($app) {
    Add-Type -Namespace Dna -Name Win -ErrorAction SilentlyContinue -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr h, out uint p);'
    $id = [uint32]0
    [void][Dna.Win]::GetWindowThreadProcessId([System.IntPtr][int]$app.Hwnd, [ref]$id)
    $id
}
function Crlf([string]$path) { ([IO.File]::ReadAllText($path) -replace "`r?`n", "`r`n") }
switch ($Action) {
    'build' {
        $book = $excel.Workbooks.Add()
        try {
            $project = $book.VBProject
            $temp = Join-Path $OutDirectory 'import'
            New-Item -ItemType Directory -Force -Path $temp | Out-Null
            foreach ($name in 'DnaExamples.bas', 'DnaSimple.bas', 'DnaCounter.cls') {
                $copy = Join-Path $temp $name
                [IO.File]::WriteAllText($copy, (Crlf (Join-Path $PSScriptRoot $name)), [Text.Encoding]::GetEncoding(1252))
                Step "import $name" { [void]$project.VBComponents.Import($copy) }
            }
            Step 'ThisWorkbook code' {
                $module = $project.VBComponents.Item('ThisWorkbook').CodeModule
                if ($module.CountOfLines -gt 0) { $module.DeleteLines(1, $module.CountOfLines) }
                $module.AddFromString((Crlf (Join-Path $PSScriptRoot 'ThisWorkbook.txt')))
            }
            Step 'Scripting reference' { [void]$project.References.AddFromGuid('{420B2830-E718-11CF-893D-00A0C9054228}', 1, 0) }
            $refs = @(); foreach ($r in $project.References) { $refs += [ordered]@{ name = $r.Name; guid = $r.Guid; major = $r.Major; minor = $r.Minor; builtin = $r.BuiltIn; broken = $r.IsBroken } }
            $result.references = $refs
            $components = @(); foreach ($c in $project.VBComponents) { $components += [ordered]@{ name = $c.Name; type = $c.Type; lines = $c.CodeModule.CountOfLines } }
            $result.components = $components
            Step 'save xlsm' { $book.SaveAs((Join-Path $OutDirectory 'DnaExamples.xlsm'), 52) }
            Step 'save xlam' { $book.SaveAs((Join-Path $OutDirectory 'DnaExamples.xlam'), 55) }
        } finally { Step 'close' { $book.Close($false) } }
        # DnaSimple.xlsm: the plain worksheet functions alone (no Range parameters, no references
        # beyond Excel's defaults), the macro workbook DNA Calc's file-project route can compile.
        $simple = $excel.Workbooks.Add()
        try {
            $copy = Join-Path (Join-Path $OutDirectory 'import') 'DnaSimple.bas'
            [IO.File]::WriteAllText($copy, (Crlf (Join-Path $PSScriptRoot 'DnaSimple.bas')), [Text.Encoding]::GetEncoding(1252))
            Step 'import DnaSimple.bas alone' { [void]$simple.VBProject.VBComponents.Import($copy) }
            Step 'save simple xlsm' { $simple.SaveAs((Join-Path $OutDirectory 'DnaSimple.xlsm'), 52) }
        } finally { Step 'close simple' { $simple.Close($false) } }
        foreach ($name in 'DnaExamples.xlsm', 'DnaExamples.xlam', 'DnaSimple.xlsm') {
            $path = Join-Path $OutDirectory $name
            if (Test-Path -LiteralPath $path) { $result.rows += [ordered]@{ file = $name; sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant(); bytes = (Get-Item -LiteralPath $path).Length } }
        }
    }
    'cases' {
        $list = Get-Content -LiteralPath $Cases -Raw -Encoding utf8 | ConvertFrom-Json
        $opened = $null; $book = $null
        try {
            # Workbook_Open runs only with application events on, as in an interactive Excel.
            $excel.EnableEvents = $true
            $opened = $excel.Workbooks.Open($AddIn, 0, $true)
            $result.addin = [ordered]@{ name = $opened.Name; is_addin = $opened.IsAddin; full_name = $opened.FullName }
            # -InBook: the formulas go into the opened macro workbook's own first sheet, where its
            # functions are called unqualified (an .xlsm); otherwise into a new workbook (an add-in).
            if ($InBook) { $sheet = $opened.Worksheets.Item(1) } else { $book = $excel.Workbooks.Add(); $sheet = $book.Worksheets.Item(1) }
            $result.sheet = $sheet.Name
            foreach ($case in $list) {
                $row = [ordered]@{ formula = $case.formula }
                $cell = $sheet.Range($FormulaCell)
                try {
                    $cell.Formula2 = $case.formula
                    $excel.Calculate()
                    $value = $cell.Value2
                    $row.excel = Describe $value
                    $row.text = [string]$cell.Text
                } catch { $row.error = $_.Exception.Message }
                $cell.ClearContents()
                $result.rows += $row
            }
        } finally {
            if ($book) { Step 'close book' { $book.Close($false) } }
            if ($opened) { Step 'close add-in' { $opened.Close($false) } }
        }
        # The answers as the controller records of crates/calc-xll-reference/run.ps1 hold them,
        # which `cargo xtask app xll-accept onecalc` compares the page with.
        $names = @{ -2146826288 = '#NULL!'; -2146826281 = '#DIV/0!'; -2146826273 = '#VALUE!'; -2146826265 = '#REF!'; -2146826259 = '#NAME?'; -2146826252 = '#NUM!'; -2146826246 = '#N/A' }
        $records = foreach ($row in $result.rows) {
            if (-not $row.Contains('excel')) { continue }
            $value = if ($row.excel.ContainsKey('error_code')) { @{ error = $names[[int]$row.excel.error_code] } } elseif ($row.excel.blank) { $null } else { $row.excel.value }
            [ordered]@{ operation = $row.formula; value = $value } | ConvertTo-Json -Compress -Depth 4
        }
        $records | Set-Content -LiteralPath (Join-Path $OutDirectory 'controller.jsonl') -Encoding utf8NoBOM
    }
    'activeobjects' {
        $source = @'
Public Function ActName() As String
    ActName = ActiveSheet.Name
End Function
Public Function ActType() As String
    ActType = TypeName(ActiveSheet)
End Function
Public Function ActBook() As String
    ActBook = ActiveWorkbook.Name
End Function
Public Function SelType() As String
    SelType = TypeName(Selection)
End Function
Public Function SelAddr() As String
    SelAddr = Selection.Address
End Function
Public Function SelCount() As Long
    SelCount = Selection.Count
End Function
Public Function SelSheet() As String
    SelSheet = Selection.Worksheet.Name
End Function
Public Function ActRead() As Variant
    ActRead = ActiveSheet.Range("B2").Value
End Function
Public Function ActCount() As Long
    ActCount = ActiveSheet.UsedRange.Count
End Function
Public Function CallerAddr() As String
    CallerAddr = Application.Caller.Address
End Function
Public Function ActCellAddr() As String
    ActCellAddr = ActiveCell.Address
End Function
Public Function ActWindow() As String
    ActWindow = TypeName(ActiveWindow)
End Function
Public Function TrySelectionWrite() As Long
    On Error Resume Next
    Selection.Value = 1
    TrySelectionWrite = Err.Number
End Function
Public Function TryActiveWrite() As Long
    On Error Resume Next
    ActiveSheet.Range("Z99").Value = 1
    TryActiveWrite = Err.Number
End Function
'@
        $names = 'ActName','ActType','ActBook','SelType','SelAddr','SelCount','SelSheet','ActRead','ActCount','CallerAddr','ActCellAddr','ActWindow','TrySelectionWrite','TryActiveWrite'
        $book = $excel.Workbooks.Add()
        try {
            $project = $book.VBProject
            $module = $project.VBComponents.Add(1)
            $module.Name = 'Probe'
            $module.CodeModule.AddFromString($source)
            $one = $book.Worksheets.Item(1); $one.Name = 'First'
            $two = $book.Worksheets.Add([Type]::Missing, $one); $two.Name = 'Second'
            $one.Range('B2').Value2 = 11; $two.Range('B2').Value2 = 22
            $two.Range('A5').Value2 = 5
            $result.windows = @(); foreach ($w in $book.Windows) { $result.windows += [ordered]@{ caption = $w.Caption; visible = $w.Visible } }
            # Scenario: where the formulas are entered (their sheet and cell), which sheet is the
            # user's active sheet and what the user has selected there.
            $scenarios = @(
                @{ id = 'same-sheet-selection-elsewhere'; at = 'First'; cell = 'H1'; active = 'First'; select = 'B2:C3' },
                @{ id = 'other-sheet-active'; at = 'Second'; cell = 'H1'; active = 'First'; select = 'B2:C3' },
                @{ id = 'active-second-selection-d4'; at = 'First'; cell = 'H1'; active = 'Second'; select = 'D4' },
                @{ id = 'selection-is-the-caller'; at = 'First'; cell = 'H1'; active = 'First'; select = 'H1' }
            )
            foreach ($scenario in $scenarios) {
                $sheetAt = $book.Worksheets.Item($scenario.at)
                $activeSheet = $book.Worksheets.Item($scenario.active)
                Step "activate $($scenario.id)" { $activeSheet.Activate(); $activeSheet.Range($scenario.select).Select() | Out-Null }
                $result.rows += [ordered]@{ scenario = $scenario.id; active_sheet = [string]$excel.ActiveSheet.Name; selection = [string]$excel.Selection.Address($false, $false); caller_cell = "$($scenario.at)!$($scenario.cell)" }
                foreach ($name in $names) {
                    $row = [ordered]@{ scenario = $scenario.id; formula = "=$name()" }
                    $cell = $sheetAt.Range($scenario.cell)
                    try {
                        $cell.Formula2 = "=$name()"
                        $excel.CalculateFull()
                        $row.excel = Describe $cell.Value2
                        $row.text = [string]$cell.Text
                    } catch { $row.error = $_.Exception.Message }
                    $cell.ClearContents()
                    $result.rows += $row
                }
            }
        } finally { Step 'close' { $book.Close($false) } }
    }
    'project' {
        $manifest = Get-Content -LiteralPath $Project -Raw -Encoding utf8 | ConvertFrom-Json
        $base = Split-Path -Parent (Resolve-Path -LiteralPath $Project)
        $list = Get-Content -LiteralPath (Join-Path $base $manifest.cases) -Raw -Encoding utf8 | ConvertFrom-Json
        $names = @{ -2146826288 = '#NULL!'; -2146826281 = '#DIV/0!'; -2146826273 = '#VALUE!'; -2146826265 = '#REF!'; -2146826259 = '#NAME?'; -2146826252 = '#NUM!'; -2146826246 = '#N/A' }
        $book = $excel.Workbooks.Add()
        try {
            $temp = Join-Path $OutDirectory 'import'
            New-Item -ItemType Directory -Force -Path $temp | Out-Null
            foreach ($module in $manifest.modules) {
                $copy = Join-Path $temp (Split-Path -Leaf $module.path)
                [IO.File]::WriteAllText($copy, (Crlf (Join-Path $base $module.path)), [Text.Encoding]::GetEncoding(1252))
                Step "import $($module.path)" { [void]$book.VBProject.VBComponents.Import($copy) }
            }
            foreach ($reference in @($manifest.references)) {
                if ($reference) { Step "reference $($reference.name)" { [void]$book.VBProject.References.AddFromGuid($reference.guid, $reference.major, $reference.minor) } }
            }
            $components = @(); foreach ($c in $book.VBProject.VBComponents) { $components += [ordered]@{ name = $c.Name; type = $c.Type; lines = $c.CodeModule.CountOfLines } }
            $result.components = $components
            $sheet = $book.Worksheets.Item(1)
            $result.sheet = $sheet.Name
            $answers = @()
            foreach ($case in $list) {
                $row = [ordered]@{ formula = $case.formula }
                $cell = $sheet.Range($FormulaCell)
                $value = $null
                try {
                    $cell.Formula2 = $case.formula
                    $excel.Calculate()
                    $value = $cell.Value2
                    $row.excel = Describe $value
                    $row.text = [string]$cell.Text
                } catch { $row.error = $_.Exception.Message }
                $cell.ClearContents()
                $result.rows += $row
                if ($row.Contains('excel')) {
                    $v = if ($row.excel.ContainsKey('error_code')) { @{ error = $names[[int]$row.excel.error_code] } } elseif ($row.excel.blank) { $null } else { $row.excel.value }
                    $answers += [ordered]@{ formula = $case.formula; value = $v }
                } else { $answers += [ordered]@{ formula = $case.formula; value = $null; excel_error = $row.error } }
            }
            ConvertTo-Json -InputObject @($answers) -Depth 6 | Set-Content -LiteralPath (Join-Path $OutDirectory 'cases.json') -Encoding utf8NoBOM
        } finally { Step 'close' { $book.Close($false) } }
    }
    'inbook' {
        $manifest = Get-Content -LiteralPath $Project -Raw -Encoding utf8 | ConvertFrom-Json
        $base = Split-Path -Parent (Resolve-Path -LiteralPath $Project)
        $list = Get-Content -LiteralPath (Join-Path $base $manifest.cases) -Raw -Encoding utf8 | ConvertFrom-Json
        $book = $excel.Workbooks.Add()
        try {
            $temp = Join-Path $OutDirectory 'import'
            New-Item -ItemType Directory -Force -Path $temp | Out-Null
            foreach ($module in $manifest.modules) {
                $copy = Join-Path $temp (Split-Path -Leaf $module.path)
                [IO.File]::WriteAllText($copy, (Crlf (Join-Path $base $module.path)), [Text.Encoding]::GetEncoding(1252))
                Step "import $($module.path)" { [void]$book.VBProject.VBComponents.Import($copy) }
            }
            foreach ($reference in @($manifest.references)) {
                if ($reference) { Step "reference $($reference.name)" { [void]$book.VBProject.References.AddFromGuid($reference.guid, $reference.major, $reference.minor) } }
            }
            $sheet = $book.Worksheets.Item(1)
            $result.sheet = $sheet.Name
            $i = 0
            foreach ($case in $list) {
                $cell = $sheet.Cells.Item(1, 1 + 8 * $i)
                $row = [ordered]@{ index = $i; address = [string]$cell.Address($false, $false); formula = $case.formula }
                try { $cell.Formula2 = $case.formula } catch { $row.error = $_.Exception.Message }
                $result.rows += $row
                $i++
            }
            $excel.CalculateFull()
            foreach ($row in $result.rows) {
                if ($row.Contains('error')) { continue }
                $cell = $sheet.Range($row.address)
                try {
                    $row.excel = Describe $cell.Value2
                    $row.text = [string]$cell.Text
                    $row.number_format = [string]$cell.NumberFormat
                } catch { $row.error = $_.Exception.Message }
            }
            Step 'save xlsm' { $book.SaveAs((Join-Path $OutDirectory ($manifest.id + '.xlsm')), 52) }
        } finally { Step 'close' { $book.Close($false) } }
        $path = Join-Path $OutDirectory ($manifest.id + '.xlsm')
        if (Test-Path -LiteralPath $path) { $result.file = [ordered]@{ file = ($manifest.id + '.xlsm'); sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant(); bytes = (Get-Item -LiteralPath $path).Length } }
    }
    'language' {
        $manifest = Get-Content -LiteralPath $Project -Raw -Encoding utf8 | ConvertFrom-Json
        $base = Split-Path -Parent (Resolve-Path -LiteralPath $Project)
        $temp = Join-Path $OutDirectory 'import'
        New-Item -ItemType Directory -Force -Path $temp | Out-Null
        $guard = Start-DialogGuard (Excel-ProcessId $excel)
        try {
            foreach ($item in $manifest.items) {
                $record = [ordered]@{ id = $item.id }
                Add-Content -LiteralPath (Join-Path $OutDirectory 'language-progress.jsonl') -Value ('{"starting":"' + $item.id + '"}') -Encoding utf8NoBOM
                $book = $excel.Workbooks.Add()
                try {
                    foreach ($path in $manifest.helper, $item.module) {
                        $copy = Join-Path $temp (Split-Path -Leaf $path)
                        [IO.File]::WriteAllText($copy, (Crlf (Join-Path $base $path)), [Text.Encoding]::GetEncoding(1252))
                        try { [void]$book.VBProject.VBComponents.Import($copy) } catch { $record.import_error = $_.Exception.Message }
                    }
                    $seen = $guard.State.dialogs.Count
                    try { $record.answer = [string]$excel.Run("'" + $book.Name + "'!DnaRunAll") } catch { $record.run_error = $_.Exception.Message }
                    Start-Sleep -Milliseconds 300
                    $record.dialogs = @($guard.State.dialogs | Select-Object -Skip $seen)
                } finally { Step "close $($item.id)" { $book.Close($false) } }
                $result.rows += $record
                # One line per finished item, so a stuck item shows where the batch stopped.
                ($record | ConvertTo-Json -Compress -Depth 5) | Add-Content -LiteralPath (Join-Path $OutDirectory 'language-progress.jsonl') -Encoding utf8NoBOM
            }
        } finally { $result.guard = Stop-DialogGuard $guard }
    }
    'reopen' {
        $logs = Join-Path $env:TEMP 'error*.xml'
        $excel.EnableEvents = $true
        foreach ($file in $Files) {
            $row = [ordered]@{ file = $file }
            $before = @(Get-ChildItem -Path $logs -ErrorAction SilentlyContinue | ForEach-Object FullName)
            $book = $null
            try {
                $book = $excel.Workbooks.Open($file, 0, $true)
                $row.name = $book.Name
                $row.repaired = $book.Name -like '*Repaired*'
                $row.has_vb_project = $book.HasVBProject
                $sheet = $book.Worksheets.Item(1)
                $cell = $sheet.Range($FormulaCell)
                $row.formula = [string]$cell.Formula2
                $row.cached = Describe $cell.Value2
                $excel.CalculateFull()
                $row.excel = Describe $cell.Value2
                $row.text = [string]$cell.Text
            } catch { $row.error = $_.Exception.Message }
            finally { if ($book) { try { $book.Close($false) } catch { $row.close_error = $_.Exception.Message } } }
            $after = @(Get-ChildItem -Path $logs -ErrorAction SilentlyContinue | ForEach-Object FullName)
            $row.repair_logs = @($after | Where-Object { $_ -notin $before })
            $result.rows += $row
        }
    }
}
$result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutDirectory "$Action.json") -Encoding utf8NoBOM
