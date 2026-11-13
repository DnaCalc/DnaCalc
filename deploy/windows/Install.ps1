# Online per-user setup; the MSIs themselves never download or execute custom actions.
# Runtime distribution: https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution
[CmdletBinding()]
param(
    [switch]$RuntimeOnly,
    [ValidateSet('DNA-Calc', 'DNA-OneCalc', 'Both')][string]$Product = 'Both'
)
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitProcess) { throw 'Use 64-bit PowerShell.' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run setup as the installing user, without elevation.'
}

function Test-WebView2 {
    $client = 'Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    foreach ($key in @("HKCU:\Software\$client", "HKLM:\SOFTWARE\WOW6432Node\$client")) {
        $pv = (Get-ItemProperty -LiteralPath $key -Name pv -ErrorAction SilentlyContinue).pv
        $parsed = $null
        if ([Version]::TryParse([string]$pv, [ref]$parsed) -and $parsed -gt [Version]'0.0.0.0') { return $true }
    }
    return $false
}

if (-not (Test-WebView2)) {
    $work = Join-Path ([IO.Path]::GetTempPath()) ('DNA-WebView2-' + [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $work | Out-Null
    try {
        $setup = Join-Path $work 'MicrosoftEdgeWebview2Setup.exe'
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $setup
        $signature = Get-AuthenticodeSignature -LiteralPath $setup
        if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
            throw 'The WebView2 bootstrapper must have a valid Microsoft signature.'
        }
        $process = Start-Process -FilePath $setup -ArgumentList '/silent /install' -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "WebView2 bootstrapper exit code $($process.ExitCode)" }
        if (-not (Test-WebView2)) { throw 'WebView2 is still unavailable after bootstrap.' }
    } finally {
        Remove-Item -LiteralPath $work -Recurse -Force
    }
}
if ($RuntimeOnly) { return }

$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'installer-manifest.json') -Raw | ConvertFrom-Json
foreach ($entry in $manifest.products) {
    if ($Product -ne 'Both' -and $entry.folder -ne $Product) { continue }
    $msi = Join-Path $PSScriptRoot $entry.msi
    $record = @($manifest.artifacts | Where-Object name -eq $entry.msi)
    if ($record.Count -ne 1 -or (Get-FileHash -LiteralPath $msi -Algorithm SHA256).Hash -ne $record[0].sha256) {
        throw "MSI hash differs from installer-manifest.json: $msi"
    }
    $log = Join-Path ([IO.Path]::GetTempPath()) ($entry.folder + '-' + [Guid]::NewGuid() + '.log')
    # MSI requires quotes when clearing a public property: ALLUSERS="".
    $process = Start-Process msiexec.exe -ArgumentList @('/i', "`"$msi`"", 'ALLUSERS=""', 'MSIINSTALLPERUSER=1', '/passive', '/norestart', '/L*v', "`"$log`"") -Wait -PassThru
    if ($process.ExitCode -notin @(0, 3010)) { throw "MSI exit code $($process.ExitCode); log: $log" }
    Write-Host "Installed $($entry.folder); log: $log"
}
