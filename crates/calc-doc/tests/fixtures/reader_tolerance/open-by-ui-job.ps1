# What Excel shows when it opens each authored workbook the way File > Open does: a plain Excel
# process of our own (not the session host's automation instance), started with the file on the
# command line, on the private desktop. Run through tools/excel-session/on-private-desktop.ps1 with
# -SessionRoot set to a folder that is a parent of every file passed (the guard recognises the
# Excel process by that path in its command line):
#   pwsh tools/excel-session/on-private-desktop.ps1 -Command "& open-by-ui-job.ps1 -Files <files> -OutDirectory <out>"
#     -Log <log> -GuardOut <json> -SessionRoot <folder>
# For every file: each dialog the process shows is recorded (window title, every text and button name,
# found with UI Automation) and dismissed through UI Automation (Yes, else OK, else Close); then the
# workbook is read back through the process's own object model (bound through its EXCEL7 window, the
# application's process ID checked against the one started) and the process is closed.
param([string[]]$Files, [string]$OutDirectory, [int]$QuietSeconds = 4, [int]$LimitSeconds = 90)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
using System; using System.Collections.Generic; using System.Runtime.InteropServices; using System.Text;
public static class DnaUiOpen {
 public delegate bool P(IntPtr h, IntPtr s);
 [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr d, P cb, IntPtr s);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
 [DllImport("oleacc.dll")] static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint id, ref Guid iid, [MarshalAs(UnmanagedType.IDispatch)] out object obj);
 public static int Pid(int hwnd) { uint p; GetWindowThreadProcessId(new IntPtr(hwnd), out p); return (int)p; }
 public static IntPtr Excel7Under(IntPtr top) { IntPtr found = IntPtr.Zero;
  EnumChildWindows(top, (k, x) => { var c = new StringBuilder(64); GetClassName(k, c, 64); if (c.ToString() == "EXCEL7") { found = k; return false; } return true; }, IntPtr.Zero); return found; }
 public static object NativeOm(IntPtr hwnd) { Guid iid = new Guid("00020400-0000-0000-C000-000000000046"); object o; int hr = AccessibleObjectFromWindow(hwnd, 0xFFFFFFF0, ref iid, out o); return hr == 0 ? o : null; }
}
'@
New-Item -ItemType Directory -Force -Path $OutDirectory | Out-Null
$A = [System.Windows.Automation.AutomationElement]
$exe = 'C:\Program Files\Microsoft Office\root\Office16\EXCEL.EXE'
function Cell($c) {
    $row = [ordered]@{ address = $c.Address($false, $false); formula = [string]$c.Formula; value = $c.Value2; has_array = [bool]$c.HasArray }
    if ($c.HasArray) { $row.array_range = $c.CurrentArray.Address($false, $false); $row.array_formula = [string]$c.FormulaArray }
    $row
}
function Windows-Of([int]$processId) {
    # The process's top-level windows, and the windows its main window owns (Excel's dialogs are owned by it, and UI Automation lists them there).
    $cond = New-Object System.Windows.Automation.PropertyCondition($A::ProcessIdProperty, $processId)
    $top = @($A::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $cond))
    $owned = @()
    foreach ($w in $top) {
        if ($w.Current.ClassName -eq 'XLMAIN') {
            $windowType = New-Object System.Windows.Automation.PropertyCondition($A::ControlTypeProperty, [System.Windows.Automation.ControlType]::Window)
            $owned += @($w.FindAll([System.Windows.Automation.TreeScope]::Children, $windowType))
        }
    }
    $top + $owned
}
$results = @()
foreach ($file in $Files) {
    $record = [ordered]@{ file = [IO.Path]::GetFileName($file); opened = $false; dialogs = @() }
    $proc = Start-Process -FilePath $exe -ArgumentList @('/x', ('"' + $file + '"')) -PassThru
    $record.pid = $proc.Id
    $started = Get-Date; $lastDialog = Get-Date; $mainSeen = $false
    try {
        while (((Get-Date) - $started).TotalSeconds -lt $LimitSeconds) {
            if ($proc.HasExited) { $record.exited_early = $true; break }
            $quiet = $true
            foreach ($w in (Windows-Of $proc.Id)) {
                $cls = $w.Current.ClassName
                if ($cls -eq 'XLMAIN') { $mainSeen = $true; continue }
                if ($cls -like 'UAC*' -or $cls -eq 'EXCEL7' -or $cls -eq 'XLDESK' -or -not $cls) { continue }
                $quiet = $false; $lastDialog = Get-Date
                $texts = @(); $buttons = @{}
                foreach ($d in $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {
                    $t = $d.Current.ControlType.ProgrammaticName
                    if ($t -eq 'ControlType.Button') { $buttons[$d.Current.Name] = $d } elseif ($d.Current.Name) { $texts += ($t + ': ' + $d.Current.Name) }
                }
                $choice = $null
                foreach ($name in 'Yes', 'OK', 'Close') { if ($buttons.ContainsKey($name)) { $choice = $name; break } }
                $record.dialogs += , ([ordered]@{ utc = [DateTime]::UtcNow.ToString('o'); class = $cls; title = $w.Current.Name; texts = $texts; buttons = @($buttons.Keys); clicked = $choice })
                if ($choice) { ([System.Windows.Automation.InvokePattern]$buttons[$choice].GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke(); Start-Sleep -Milliseconds 800 }
            }
            if ($mainSeen -and $quiet -and ((Get-Date) - $lastDialog).TotalSeconds -ge $QuietSeconds) { break }
            Start-Sleep -Milliseconds 250
        }
        $main = (Windows-Of $proc.Id) | Where-Object { $_.Current.ClassName -eq 'XLMAIN' } | Select-Object -First 1
        $record.main_window_title = if ($main) { $main.Current.Name } else { $null }
        $app = $null
        if ($main) {
            $e7 = [DnaUiOpen]::Excel7Under([IntPtr][int]$main.Current.NativeWindowHandle)
            $window = if ($e7 -ne [IntPtr]::Zero) { [DnaUiOpen]::NativeOm($e7) } else { $null }
            if ($window) { $app = $window.Application }
        }
        if ($app) {
            $appPid = [DnaUiOpen]::Pid([int]$app.Hwnd)
            if ($appPid -ne $proc.Id) { throw "Bound Excel application belongs to process $appPid, not the process started ($($proc.Id))" }
            $book = $null
            if ($app.Workbooks.Count -gt 0) { $book = $app.Workbooks.Item(1) }
            if ($book) {
                $record.opened = $true; $record.workbook_name = [string]$book.Name; $record.sheets = @()
                foreach ($sheet in $book.Sheets) {
                    $s = [ordered]@{ index = $sheet.Index; name = [string]$sheet.Name; visible = [int]$sheet.Visible; cells = @() }
                    try { foreach ($c in $sheet.UsedRange.Cells) { if ($c.Formula -ne '' -or $null -ne $c.Value2) { $s.cells += , (Cell $c) } } } catch { $s.cells_error = $_.Exception.Message }
                    $record.sheets += , $s
                }
                # What Excel writes for the workbook as it now stands (repairs applied): sheetIds and all.
                $saved = Join-Path $OutDirectory ('saved-' + [IO.Path]::GetFileName($file))
                try { $book.SaveCopyAs($saved); $record.saved_copy = [IO.Path]::GetFileName($saved) } catch { $record.save_error = $_.Exception.Message }
            } else { $record.workbooks_open = 0 }
            try { $app.DisplayAlerts = $false; if ($book) { $book.Close($false) }; $app.Quit() } catch { }
        }
    } catch { $record.error = $_.Exception.Message }
    finally {
        if (-not $proc.WaitForExit(8000)) { $proc.Kill() }   # only the process started above
    }
    $results += , $record
}
$results | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $OutDirectory 'excel-open-by-ui.json') -Encoding utf8NoBOM
