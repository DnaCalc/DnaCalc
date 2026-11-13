# dc-1nbp.6 (14.e): Excel session job for the object-model probes E6, E11 and E1.
# Receives the owned Excel from tools/excel-session/host.ps1. Copies the built X19Com add-in
# (fixture/X19Com.csproj with ComProbe.cs and OmProbe.cs) into the run directory, registers it,
# runs the three Excel-DNA commands with Application.Run and collects their om-*.tsv files.
# Also records the out-of-process late-bound view (this PowerShell job is itself a COM client
# of the owned Excel) for E1's identity question.
param($OwnedExcel, [string]$Request, [string]$Result)
$ErrorActionPreference = 'Stop'
$req = Get-Content -LiteralPath $Request -Raw | ConvertFrom-Json
$run = [IO.Path]::GetFullPath($req.run_dir)
$addin = Join-Path $run 'addin'
New-Item -ItemType Directory -Force -Path $addin | Out-Null
Get-ChildItem -LiteralPath $req.build_dir -File | Copy-Item -Destination $addin
$xll = Join-Path $addin 'X19Com-AddIn64.xll'
$excel = $OwnedExcel
$excelPid = 0
$hwnd = [int64]$excel.Hwnd
Add-Type -Namespace OmProbe -Name W -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);'
[void][OmProbe.W]::GetWindowThreadProcessId([IntPtr]$hwnd, [ref]$excelPid)
$proc = Get-Process -Id $excelPid
$env = [ordered]@{
    captured_utc = [DateTime]::UtcNow.ToString('O'); pid = $excelPid; trial = $req.trial
    excel_version = $excel.Version; excel_build = $excel.Build
    excel_file_version = $proc.MainModule.FileVersionInfo.FileVersion; excel_path = $proc.Path
    operating_system = $excel.OperatingSystem; calculation = $excel.Calculation
    display_alerts = $excel.DisplayAlerts; decimal_separator = $excel.DecimalSeparator
    use_system_separators = $excel.UseSystemSeparators; country = $excel.International(1); automation_security = $excel.AutomationSecurity
    addin_sha256 = (Get-FileHash -LiteralPath $xll).Hash; x19com_dll_sha256 = (Get-FileHash -LiteralPath (Join-Path $addin 'X19Com.dll')).Hash
}

# Out-of-process late-bound client: two acquisitions of Application.Range("A2").
$oop = [ordered]@{}
$book = $excel.Workbooks.Add()
$book.Worksheets.Item(1).Range('A1:A3').Value2 = 10
$a = $excel.Range('A2'); $b = $excel.Range('A2')
$ua = [Runtime.InteropServices.Marshal]::GetIUnknownForObject($a); $ub = [Runtime.InteropServices.Marshal]::GetIUnknownForObject($b)
$oop['ReferenceEquals(a, b)'] = [object]::ReferenceEquals($a, $b)
$oop['IUnknown(a) == IUnknown(b)'] = ($ua -eq $ub)
[void][Runtime.InteropServices.Marshal]::Release($ua); [void][Runtime.InteropServices.Marshal]::Release($ub)
$c = $book.Worksheets.Item(1).Cells.Item(2, 1)
$uc = [Runtime.InteropServices.Marshal]::GetIUnknownForObject($c); $ua = [Runtime.InteropServices.Marshal]::GetIUnknownForObject($a)
$oop['IUnknown(a) == IUnknown(Cells(2,1))'] = ($ua -eq $uc)
[void][Runtime.InteropServices.Marshal]::Release($ua); [void][Runtime.InteropServices.Marshal]::Release($uc)
$book.Close($false)

$started = [DateTime]::UtcNow
$registered = [bool]$excel.RegisterXLL($xll)
$probes = [ordered]@{}
if ($registered) {
    foreach ($name in 'E6', 'E11', 'E1') {
        $t0 = [DateTime]::UtcNow
        try { [void]$excel.Run("OM_$name"); $err = $null } catch { $err = $_.Exception.Message }
        $file = Join-Path $addin ('om-' + $name.ToLowerInvariant() + '.tsv')
        $probes[$name] = [ordered]@{ run_error = $err; seconds = ([DateTime]::UtcNow - $t0).TotalSeconds; output = (Test-Path -LiteralPath $file) }
        if (Test-Path -LiteralPath $file) { Copy-Item -LiteralPath $file -Destination $run }
    }
}
$env['display_alerts_after'] = $excel.DisplayAlerts
$out = [ordered]@{ fatal = $null; environment = $env; registered = $registered; out_of_process_late_bound = $oop; probes = $probes; seconds = ([DateTime]::UtcNow - $started).TotalSeconds }
$out | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Result -Encoding utf8NoBOM
$out | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $run 'job-result.json') -Encoding utf8NoBOM
