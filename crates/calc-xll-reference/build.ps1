param([switch]$NativeOnly)
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$out = Join-Path $root 'build'
New-Item -ItemType Directory -Force $out | Out-Null
$vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Visual C++ x64 tools are required.' }
$vcvars = Join-Path $vs 'VC/Auxiliary/Build/vcvars64.bat'
$source = Join-Path $root 'native/trace.cpp'
$exe = Join-Path $out 'trace.exe'
$obj = Join-Path $out 'trace.obj'
$ownedSource = Join-Path $root 'native/owned.cpp'
$ownedObj = Join-Path $out 'owned.obj'
$ownedXll = Join-Path $out 'NativeOwner64.xll'
$buildCommand = Join-Path $out 'compile.cmd'
@"
@echo off
set "PATH=$([IO.Path]::GetDirectoryName($vswhere));%PATH%"
call "$vcvars" >nul
if errorlevel 1 exit /b 1
cl /nologo /std:c++17 /EHsc /W4 /O2 /DUNICODE /D_UNICODE "$source" /Fo"$obj" /Fe"$exe" /link /INCREMENTAL:NO
if errorlevel 1 exit /b 1
cl /nologo /std:c++17 /EHsc /W4 /O2 /LD /DUNICODE /D_UNICODE "$ownedSource" /Fo"$ownedObj" /Fe"$ownedXll" /link /INCREMENTAL:NO
"@ | Set-Content -LiteralPath $buildCommand -Encoding ascii
& cmd.exe /d /c $buildCommand
if ($LASTEXITCODE -ne 0) { throw 'Native tracer build failed.' }
if (-not $NativeOnly) {
    & dotnet build (Join-Path $root 'fixture/X0.csproj') -c Release
    if ($LASTEXITCODE -ne 0) { throw 'Fixture build failed.' }
}
