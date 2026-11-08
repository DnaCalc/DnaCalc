param(
    [Parameter(Mandatory)][string]$OutDirectory,
    [string[]]$Set,
    [string]$X1Xll,
    [string]$X3Directory = 'C:\Work\DnaCalc\codex-runs\x3-upstream',
    [string]$CalcVbaDirectory = 'C:\Work\DnaCalc\codex-runs\calcvba-24-udf-verified\generated',
    [string]$CalcVbaCases = 'C:\Work\DnaCalc\codex-runs\m4\artifacts\calcvba-udf-cases.json'
)
# Excel's answers for the OneCalc milestone 4 acceptance sets (fixture/m4-acceptance.json): one
# untraced run of run.ps1 per set, in an owned hidden Excel session (tools/excel-session). Each set
# gets a new directory under -OutDirectory with Excel's controller.jsonl. Nothing here retries a
# set; a failed set stays failed and the next set runs. The OneCalc side is
# `cargo xtask app xll-accept onecalc`, which loads the same files through the page.
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$run = Join-Path $here 'run.ps1'
$fixture = Join-Path $here 'fixture'
$sets = [ordered]@{
    'x1-fixture'     = @{ X1Xll = $X1Xll }
    'exceldna-net48' = @{ ExcelDnaDirectory = (Join-Path $fixture 'bin/Release/net48'); AddInName = 'X0' }
    'exceldna-net8'  = @{ ExcelDnaDirectory = (Join-Path $fixture 'bin/Release/net8.0-windows'); AddInName = 'X0' }
    'exceldna-aot'   = @{ ExcelDnaDirectory = (Join-Path $fixture 'bin/Release/net10.0-windows/win-x64/publish'); AddInName = 'X2Aot' }
    'exceldna-pool'  = @{ ExcelDnaDirectory = (Join-Path $fixture 'bin/Release/net48'); AddInName = 'X2Pool' }
    'xlw'            = @{ AddInDirectory = (Join-Path $X3Directory 'xlw-runtime'); XllFileName = 'xlwSample.xll'; Cases = (Join-Path $fixture 'x3-xlw-cases.json') }
    'xlw-types'      = @{ AddInDirectory = (Join-Path $X3Directory 'xlw-types-runtime'); XllFileName = 'xlwTypes.xll'; Cases = (Join-Path $fixture 'x3-xlw-types-cases.json') }
    'xloil-utils-sql' = @{ AddInDirectory = (Join-Path $X3Directory 'xloil-runtime'); XllFileName = 'xlOil.xll'; Cases = (Join-Path $fixture 'x3-xloil-cases.json') }
    'xloil-python'   = @{ AddInDirectory = (Join-Path $X3Directory 'xloil-python-runtime'); XllFileName = 'xlOil.xll'; Cases = (Join-Path $fixture 'x3-xloil-python-cases.json') }
    'calcvba-udf'    = @{ AddInDirectory = $CalcVbaDirectory; XllFileName = 'FunctionsUdf.xll'; Cases = $CalcVbaCases }
}
New-Item -ItemType Directory -Force -Path $OutDirectory | Out-Null
foreach ($name in $sets.Keys) {
    if ($Set -and $name -notin $Set) { continue }
    $directory = Join-Path $OutDirectory $name
    if (Test-Path -LiteralPath $directory) { Write-Output "$name : $directory exists; earlier evidence is never overwritten"; continue }
    $parameters = $sets[$name]
    try {
        & pwsh -NoProfile -File $run -NoTrace -RunDirectory $directory @parameters | Out-Null
        $record = Join-Path $directory 'controller.jsonl'
        Write-Output ("{0} : {1}" -f $name, $(if (Test-Path -LiteralPath $record) { "Excel answered ($((Get-Content -LiteralPath $record).Count) records)" } else { 'no controller record' }))
    } catch {
        Write-Output ("{0} : FAILED {1}" -f $name, $_.Exception.Message)
    }
}
