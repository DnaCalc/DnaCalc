param([Parameter(Mandatory)][string]$ProbeDirectory,[Parameter(Mandatory)][string]$RunId,[string]$OxVbaRoot='C:/Work/DnaCalc/OxVba')
# Reuse the P07 controller; only the evidence directory differs.
& (Join-Path $PSScriptRoot '../external-controller.ps1') @PSBoundParameters
