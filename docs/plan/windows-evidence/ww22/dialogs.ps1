param([string]$Run,[int]$Seconds=30,[string]$Action='Cancel')
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$deadline=[DateTime]::UtcNow.AddSeconds($Seconds)
while([DateTime]::UtcNow -lt $deadline) {
    try {$state=Get-Content "$Run/session/host.json" -Raw | ConvertFrom-Json} catch {Start-Sleep -Milliseconds 250;continue}
    if(!$state.excel.pid) {Start-Sleep -Milliseconds 250;continue}
    $owned=Get-Process -Id $state.excel.pid -ErrorAction Ignore
    if(!$owned -or $owned.ProcessName -ne 'EXCEL' -or $owned.StartTime.ToUniversalTime().Ticks -ne $state.excel.start_ticks) {Start-Sleep -Milliseconds 250;continue}
    $id=[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,[int]$owned.Id)
    $root=[Windows.Automation.AutomationElement]::RootElement
    foreach($win in $root.FindAll([Windows.Automation.TreeScope]::Children,$id)) {
        if($win.Current.ClassName -ne '#32770' -and $win.Current.Name -notin 'Unprotect Sheet','Protect Sheet','Microsoft Visual Basic','Microsoft Visual Basic for Applications','Microsoft Excel') {continue}
        $elements=@($win.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition))
        $buttons=@($elements | Where-Object {$_.Current.ControlType -eq [Windows.Automation.ControlType]::Button})
        $target=$buttons | Where-Object {$_.Current.Name -eq $Action} | Select-Object -First 1
        if(!$target) {continue}
        $text=@($elements | ForEach-Object {$_.Current.Name} | Where-Object {$_})
        @{pid=$owned.Id;name=$win.Current.Name;text=$text;action=$Action} | ConvertTo-Json -Compress | Add-Content "$Run/dialogs.jsonl"
        $pattern=$null
        if($target.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) {
            $pattern.Invoke()
            return
        }
        throw 'The selected dialog button did not expose InvokePattern'
    }
    Start-Sleep -Milliseconds 250
}
