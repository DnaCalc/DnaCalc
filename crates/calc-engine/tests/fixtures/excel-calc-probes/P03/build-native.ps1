param([string]$Source = '',[string]$Output = '')
$ErrorActionPreference='Stop'
if(-not $Source){$Source=Join-Path $PSScriptRoot 'native-probe.cpp'}
if(-not $Output){$Output=Join-Path $PSScriptRoot 'native-probe.xll'}
$vs = 'C:/Program Files/Microsoft Visual Studio/18/Insiders/VC/Auxiliary/Build/vcvars64.bat'
$build = Join-Path $PSScriptRoot 'build-native.cmd'
$text = '@echo off' + "`r`n" + 'call "' + $vs + '"' + "`r`n" + 'cl /nologo /LD /EHsc /std:c++17 /O2 "' + $Source + '" /link /OUT:"' + $Output + '"' + "`r`n"
[IO.File]::WriteAllText($build,$text)
Push-Location $PSScriptRoot
try { & cmd.exe /c $build; if($LASTEXITCODE -ne 0){throw "Native build failed $LASTEXITCODE"} }
finally {Pop-Location}
