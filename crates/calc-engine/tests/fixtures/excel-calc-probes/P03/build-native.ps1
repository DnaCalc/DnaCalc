$ErrorActionPreference='Stop'
$vs = 'C:/Program Files/Microsoft Visual Studio/18/Insiders/VC/Auxiliary/Build/vcvars64.bat'
$build = Join-Path $PSScriptRoot 'build-native.cmd'
$text = '@echo off' + "`r`n" + 'call "' + $vs + '"' + "`r`n" + 'cl /nologo /LD /EHsc /std:c++17 /O2 "' + (Join-Path $PSScriptRoot 'native-probe.cpp') + '" /link /OUT:"' + (Join-Path $PSScriptRoot 'native-probe.xll') + '"' + "`r`n"
[IO.File]::WriteAllText($build,$text)
Push-Location $PSScriptRoot
try { & cmd.exe /c $build; if($LASTEXITCODE -ne 0){throw "Native build failed $LASTEXITCODE"} }
finally {Pop-Location}
