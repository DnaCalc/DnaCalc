# The harness the worker's main-flow tests run (src/worker_tests.rs): it shadows New-Object with a
# guard that passes every .NET object through and creates no COM object (New-Object -ComObject
# returns $global:StandinApplication when the setup gave one, and throws otherwise, so the worker
# never starts Excel), defines the stand-in builders, dot-sources the file $Setup, then calls the
# worker with -Request and -Result, the two parameters the adapter passes it, and exits with the
# worker's exit code. What it shares with the adapter's run is those parameters and the request and
# result files; capture() starts the worker as its own powershell.exe -File under run_isolated,
# which this harness does not. Lines the guard and the stand-ins print start with "STANDIN:". The
# script is ASCII and runs under Windows PowerShell 5.1.
param([string]$Worker, [string]$Request, [string]$Result, [string]$Setup)
$global:StandinApplication = $null
# What standin-excel.ps1 gives its Sheet1, saves and lists, when a setup sets them before it: the
# formula cells, the package its SaveAs writes, and the further cells SpecialCells lists.
$global:StandinCells = $null
$global:StandinSaved = $null
$global:StandinListed = $null
function global:Write-Standin([string]$Text) { [Console]::Out.WriteLine('STANDIN: ' + $Text); [Console]::Out.Flush() }
function global:New-Object {
    param(
        [Parameter(Position = 0)][string]$TypeName,
        [Parameter(Position = 1)][object[]]$ArgumentList,
        [string]$ComObject,
        [hashtable]$Property,
        [switch]$Strict
    )
    if ($PSBoundParameters.ContainsKey('ComObject')) {
        Write-Standin "New-Object -ComObject $ComObject"
        if ($null -eq $global:StandinApplication) { throw "the test's guard creates no COM object ($ComObject)" }
        return $global:StandinApplication
    }
    $made = Microsoft.PowerShell.Utility\New-Object @PSBoundParameters
    return , $made
}
# A stand-in Excel Application: a hidden window of this process stands for Excel's, so its PID is
# what the worker reports; Workbooks.Open and Quit print what they were called with.
# Each setting named in $Unreadable is one whose read fails (a getter that throws, which
# PowerShell reads as nothing) and whose write is ignored.
function global:New-StandinApplication($ComAddIns, $AddIns, [string[]]$Unreadable = @()) {
    Add-Type -AssemblyName System.Windows.Forms
    $global:StandinForm = Microsoft.PowerShell.Utility\New-Object System.Windows.Forms.Form
    $workbooks = [pscustomobject]@{}
    $workbooks | Add-Member -MemberType ScriptMethod -Name Open -Value { Write-Standin ('Workbooks.Open ' + $args[0]); throw 'the stand-in Application opens no workbook' }
    $application = [pscustomobject]@{
        Hwnd = $global:StandinForm.Handle.ToInt64()
        Visible = $true; ScreenUpdating = $true
        COMAddIns = @($ComAddIns); AddIns2 = @($AddIns); RegisteredFunctions = $null
        Workbooks = $workbooks
    }
    $settings = [ordered]@{ DisplayAlerts = $true; EnableEvents = $true; Interactive = $true; AutomationSecurity = 1 }
    foreach ($name in $settings.Keys) {
        if ($Unreadable -contains $name) {
            $application | Add-Member -MemberType ScriptProperty -Name $name -Value { throw 'the stand-in cannot read this setting' } -SecondValue { }
        } else {
            $application | Add-Member -MemberType NoteProperty -Name $name -Value $settings[$name]
        }
    }
    $application | Add-Member -MemberType ScriptMethod -Name Quit -Value { Write-Standin 'Quit' }
    return $application
}
function global:New-StandinComAddIn([string]$ProgId, $Connect) {
    $addIn = [pscustomobject]@{ ProgId = $ProgId }
    if ($null -eq $Connect) {
        $addIn | Add-Member -MemberType ScriptProperty -Name Connect -Value { throw 'the stand-in cannot read Connect' }
    } else {
        $addIn | Add-Member -MemberType NoteProperty -Name Connect -Value $Connect
    }
    return $addIn
}
function global:New-StandinAddIn([string]$FullName, [bool]$IsOpen, [bool]$Installed) {
    return [pscustomobject]@{ FullName = $FullName; IsOpen = $IsOpen; Installed = $Installed }
}
. $Setup
& $Worker -Request $Request -Result $Result
exit $LASTEXITCODE
