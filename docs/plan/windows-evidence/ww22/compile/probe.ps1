param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
$homePath=Split-Path $PSScriptRoot
foreach($name in 'standard-module-me','variable-error','nonexistent-application-member') {
    $case="dc-711ye-$name"
    if($name -eq 'nonexistent-application-member') {
        $b=Book
        $m=$b.VBProject.VBComponents.Add(1)
        $m.Name='Module1'
        $m.CodeModule.AddFromString(@"
Public Function Probe() As Variant
Probe = Application.NoSuchName(1)
End Function
"@)
    } else {$b=$excel.Workbooks.Open("$homePath/authored/inputs/$case.xlsm",0,$true)}
    try {
        $shell="$env:SystemRoot/System32/WindowsPowerShell/v1.0/powershell.exe"
        $watch=Start-Process -FilePath $shell -WindowStyle Hidden -PassThru -ArgumentList "-NoProfile -File ""$homePath/dialogs.ps1"" -Run ""$Directory"" -Seconds 15 -Action OK"
        Observe "Compile.$case" { $excel.Run("'$($b.Name)'!Module1.Probe") }
        if(!$watch.WaitForExit(18000)) {$watch.Kill()}
    } finally {$b.Close($false)}
}
