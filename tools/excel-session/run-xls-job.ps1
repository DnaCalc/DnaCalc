# Generate controlled inputs and submit one legacy-format job to the owned Excel session.
param(
    [Parameter(Mandatory=$true)][ValidateSet('compat','conversion')][string]$Kind,
    [Parameter(Mandatory=$true)][string]$Output,
    [Parameter(Mandatory=$true)][string]$InputDirectory,
    [string]$SessionDirectory = $env:DNACALC_EXCEL_SESSION
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$inputs = [IO.Path]::GetFullPath($InputDirectory)
$result = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $result) { throw "Result already exists: $result" }
[IO.Directory]::CreateDirectory($inputs) | Out-Null
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($result)) | Out-Null
$old = $env:DNACALC_XLS_SESSION_INPUTS
try {
    $env:DNACALC_XLS_SESSION_INPUTS = $inputs
    Push-Location $root
    try {
        & cargo test --locked -p calc-doc --test calc-doc-integration dc_4j0e_xls::write_legacy_excel_session_inputs -- --exact
        if ($LASTEXITCODE -ne 0) { throw "Fixture generation failed: $LASTEXITCODE" }
    } finally { Pop-Location }
} finally { $env:DNACALC_XLS_SESSION_INPUTS = $old }
$jobName = if ($Kind -eq 'compat') {'xls-compat-job.ps1'} else {'xls-conversion-job.ps1'}
$job = Join-Path $PSScriptRoot $jobName
$params = @{Cases=(Join-Path $inputs 'cases.json'); Result=$result} | ConvertTo-Json -Compress
$submitArgs = @{Script=$job; Parameters=$params; WorkingDirectory=$root; TimeoutSeconds=1800; FreshRequired=$true}
if ($SessionDirectory) { $submitArgs.SessionDirectory = $SessionDirectory }
& (Join-Path $PSScriptRoot 'submit.ps1') @submitArgs
if (-not (Test-Path -LiteralPath $result)) { throw "Job did not write $result" }
