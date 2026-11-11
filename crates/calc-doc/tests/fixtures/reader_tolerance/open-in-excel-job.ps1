# One owned-Excel job (tools/excel-session): what Excel does when it opens each authored workbook
# of this folder. Run it through on-private-desktop.ps1 so no window reaches the owner's desktop:
#   DNACALC_EXCEL_SESSION=<dir> pwsh tools/excel-session/on-private-desktop.ps1 -Command "& open-in-excel-job.ps1
#     -Files a.xlsx,b.xlsx -OutDirectory <out> [-Alerts]" -Log <log> -GuardOut <json> -SessionRoot <dir>
# Per file it records: whether Workbooks.Open returned or threw, every dialog Excel showed (text and
# buttons, captured with UI Automation on the private desktop, then dismissed through its Yes, OK or
# Close button; only windows of the owned Excel process are touched), and what the opened workbook
# holds (sheet list, then every cell of each sheet's used range: formula, value, array flag).
# -Repair passes CorruptLoad = xlRepairFile (1) to Workbooks.Open: without it an automation open refuses a package
# that Excel's own File > Open would offer to repair.
# -Alerts sets Application.DisplayAlerts and -Interactive sets Application.Interactive to true for the
# opens (the session host runs with both false and restores its baseline after the job).
param([string[]]$Files, [string]$OutDirectory, [switch]$Alerts, [switch]$Interactive, [switch]$Repair, [int]$TimeoutSeconds = 600, $OwnedExcel = $null)
$ErrorActionPreference = 'Stop'
if ($null -eq $OwnedExcel) {
    $parameters = @{ Files = @($Files | ForEach-Object { (Resolve-Path -LiteralPath $_).Path }); OutDirectory = [IO.Path]::GetFullPath($OutDirectory); Alerts = [bool]$Alerts; Interactive = [bool]$Interactive; Repair = [bool]$Repair }
    $client = (Resolve-Path (Join-Path $PSScriptRoot '../../../../../tools/excel-session/submit.ps1')).Path
    & $client -Script $PSCommandPath -Parameters ($parameters | ConvertTo-Json -Compress -Depth 4) -Environment 'reader-tolerance' -TimeoutSeconds $TimeoutSeconds -LeakEvery 1000000
    return
}
$excel = $OwnedExcel
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
if (-not ('DnaTolHwnd' -as [type])) {
    Add-Type @'
using System; using System.Runtime.InteropServices;
public static class DnaTolHwnd { [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
 public static int Pid(int hwnd) { uint p; GetWindowThreadProcessId(new IntPtr(hwnd), out p); return (int)p; } }
'@
}
$excelPid = [DnaTolHwnd]::Pid([int]$excel.Hwnd)
New-Item -ItemType Directory -Force -Path $OutDirectory | Out-Null
$dialogs = [System.Collections.ArrayList]::Synchronized((New-Object System.Collections.ArrayList))
$stop = [hashtable]::Synchronized(@{ stop = $false })
$watcher = [powershell]::Create()
[void]$watcher.AddScript({
    param($excelPid, $dialogs, $stop)
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    $A = [System.Windows.Automation.AutomationElement]
    $pidCond = New-Object System.Windows.Automation.PropertyCondition($A::ProcessIdProperty, [int]$excelPid)
    while (-not $stop.stop) {
        try {
            foreach ($w in $A::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $pidCond)) {
                $cls = $w.Current.ClassName
                if ($cls -like 'UAC*' -or $cls -eq 'XLMAIN' -or $cls -eq 'EXCEL7' -or $cls -eq 'XLDESK') { continue }
                $texts = @(); $buttons = @{}
                foreach ($d in $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {
                    $t = $d.Current.ControlType.ProgrammaticName
                    if ($t -eq 'ControlType.Button') { $buttons[$d.Current.Name] = $d } elseif ($d.Current.Name) { $texts += ($t + ': ' + $d.Current.Name) }
                }
                $choice = $null
                foreach ($name in 'Yes', 'OK', 'Close', 'Cancel') { if ($buttons.ContainsKey($name)) { $choice = $name; break } }
                [void]$dialogs.Add([ordered]@{ utc = [DateTime]::UtcNow.ToString('o'); class = $cls; title = $w.Current.Name; texts = $texts; buttons = @($buttons.Keys); clicked = $choice })
                if ($choice) { ([System.Windows.Automation.InvokePattern]$buttons[$choice].GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke() }
                Start-Sleep -Milliseconds 700
            }
        } catch { }
        Start-Sleep -Milliseconds 150
    }
}).AddArgument($excelPid).AddArgument($dialogs).AddArgument($stop)
$handle = $watcher.BeginInvoke()
function Cell($c) {
    $row = [ordered]@{ address = $c.Address($false, $false); formula = [string]$c.Formula; value = $c.Value2; has_array = [bool]$c.HasArray }
    if ($c.HasArray) { $row.array_range = $c.CurrentArray.Address($false, $false); $row.array_formula = [string]$c.FormulaArray }
    $row
}
$results = @()
try {
    $before = $excel.DisplayAlerts; $beforeInteractive = $excel.Interactive
    if ($Alerts) { $excel.DisplayAlerts = $true }
    if ($Interactive) { $excel.Interactive = $true }
    foreach ($file in $Files) {
        $dialogs.Clear()
        $record = [ordered]@{ file = [IO.Path]::GetFileName($file); corrupt_load = $(if ($Repair) { 'xlRepairFile' } else { 'xlNormalLoad' }); display_alerts = [bool]$excel.DisplayAlerts; interactive = [bool]$excel.Interactive; opened = $false; error = $null }
        $book = $null
        try {
            $book = if ($Repair) { [System.__ComObject].InvokeMember('Open', [Reflection.BindingFlags]::InvokeMethod, $null, $excel.Workbooks, @($file, 0, $true, 1), $null, $null, @('Filename', 'UpdateLinks', 'ReadOnly', 'CorruptLoad')) } else { $excel.Workbooks.Open($file, 0, $true) }
        } catch { $record.error = $_.Exception.Message; $record.hresult = ('0x{0:X8}' -f $_.Exception.HResult); if ($_.Exception.InnerException) { $record.inner_error = $_.Exception.InnerException.Message } }
        Start-Sleep -Milliseconds 1500
        $record.dialogs = @($dialogs.ToArray())
        if ($book) {
            $record.opened = $true
            $record.workbook_name = [string]$book.Name
            $record.sheets = @()
            foreach ($sheet in $book.Sheets) {
                $s = [ordered]@{ index = $sheet.Index; name = [string]$sheet.Name; visible = [int]$sheet.Visible; cells = @() }
                try { foreach ($c in $sheet.UsedRange.Cells) { if ($c.Formula -ne '' -or $null -ne $c.Value2) { $s.cells += , (Cell $c) } } } catch { $s.cells_error = $_.Exception.Message }
                $record.sheets += , $s
            }
            try { $book.Close($false) } catch { }
        }
        $results += , $record
    }
    $excel.DisplayAlerts = $before; $excel.Interactive = $beforeInteractive
} finally {
    $stop.stop = $true
    try { [void]$watcher.EndInvoke($handle) } catch { }
    $watcher.Dispose()
}
$name = 'excel-open-alerts-' + $(if ($Alerts) { 'on' } else { 'default' }) + '-interactive-' + $(if ($Interactive) { 'on' } else { 'default' }) + $(if ($Repair) { '-repair' } else { '' }) + '.json'
$results | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $OutDirectory $name) -Encoding utf8NoBOM
