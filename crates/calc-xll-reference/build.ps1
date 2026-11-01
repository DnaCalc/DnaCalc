param([switch]$NativeOnly, [switch]$X2, [switch]$X3, [string]$UpstreamDirectory)
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
if ($X3) {
    if (-not $UpstreamDirectory) { throw '-X3 needs an -UpstreamDirectory outside this repository.' }
    $upstream=[IO.Path]::GetFullPath($UpstreamDirectory)
    $repository=[IO.Path]::GetFullPath((Join-Path $root '../..')).TrimEnd('\')
    if ($upstream.StartsWith($repository+'\',[StringComparison]::OrdinalIgnoreCase) -or $upstream -eq $repository) { throw 'Upstream inputs must stay outside the repository.' }
    $null=New-Item -ItemType Directory -Force -Path $upstream
    $manifest=Get-Content -LiteralPath (Join-Path $root 'fixture/x3-upstream.json') -Raw | ConvertFrom-Json
    function Fetch([string]$Url,[string]$Path,[string]$Sha) {
        if (-not (Test-Path -LiteralPath $Path)) { Invoke-WebRequest -Uri $Url -OutFile $Path }
        if ((Get-FileHash -LiteralPath $Path).Hash -ne $Sha) { throw "Upstream digest differs: $Path" }
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $package=Join-Path $upstream 'xlw.23.10.12-rc.nupkg'
    Fetch $manifest.xlw.package_url $package $manifest.xlw.package_sha256
    $xlwPackage=Join-Path $upstream 'xlw-package'
    if (-not (Test-Path -LiteralPath $xlwPackage)) { [IO.Compression.ZipFile]::ExtractToDirectory($package,$xlwPackage) }
    $sample=Join-Path $upstream 'xlw-sample'
    $null=New-Item -ItemType Directory -Force -Path (Join-Path $sample 'generated')
    foreach ($property in $manifest.xlw.source_paths.PSObject.Properties) {
        $name=[IO.Path]::GetFileName($property.Name)
        Fetch ("https://raw.githubusercontent.com/xlw/xlw/{0}/{1}" -f $manifest.xlw.source_commit,$property.Name) (Join-Path $sample $name) $manifest.xlw.source_raw_sha256.PSObject.Properties[$property.Name].Value
    }
    & (Join-Path $xlwPackage 'native/lib/InterfaceGenerator.exe') (Join-Path $sample 'cppinterface.h') (Join-Path $sample 'generated/xlw-sample.cpp')
    if ($LASTEXITCODE -ne 0) { throw 'Original xlw interface generator failed.' }
    $sampleBuild=Join-Path $sample 'build.cmd'
@"
@echo off
set "PATH=$([IO.Path]::GetDirectoryName($vswhere));%PATH%"
call "$vcvars" >nul
if errorlevel 1 exit /b 1
cl /nologo /std:c++17 /EHsc /O2 /MD /DNDEBUG /LD /I"$xlwPackage/native/include" /I"$sample" "$sample/source.cpp" "$sample/generated/xlw-sample.cpp" /Fo"$sample/" /Fe"$sample/xlwSample.xll" /link /LTCG "$xlwPackage/native/lib/x64/xlw-vc143-mt-23_10_12.lib" user32.lib advapi32.lib version.lib
"@ | Set-Content -LiteralPath $sampleBuild -Encoding ascii
    & cmd.exe /d /c $sampleBuild
    if ($LASTEXITCODE -ne 0) { throw 'Original xlw sample build failed.' }
    $xlwRuntime=Join-Path $upstream 'xlw-runtime'
    $null=New-Item -ItemType Directory -Force -Path $xlwRuntime
    Copy-Item -LiteralPath (Join-Path $sample 'xlwSample.xll') -Destination $xlwRuntime
    foreach ($wheel in $manifest.xloil.wheels) {
        $file=Join-Path $upstream $wheel.file
        Fetch $wheel.url $file $wheel.sha256
        $directory=Join-Path $upstream $(if($wheel.file -match 'cp312'){'xloil-wheel312'}else{'xloil-wheel'})
        if (-not (Test-Path -LiteralPath $directory)) { [IO.Compression.ZipFile]::ExtractToDirectory($file,$directory) }
    }
    $pythonInfo=(& python -c 'import sys,numpy,json,pathlib; print(json.dumps({"executable":str(pathlib.Path(sys.base_prefix)/"python.exe"),"numpy_path":str(pathlib.Path(numpy.__file__).parent.parent),"numpy_version":numpy.__version__}))') | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'The Python profile needs existing x64 Python 3.12 and NumPy.' }
    foreach ($profile in 'xloil-runtime','xloil-python-runtime') {
        $runtime=Join-Path $upstream $profile
        $null=New-Item -ItemType Directory -Force -Path $runtime
        $withPython=$profile -eq 'xloil-python-runtime'
        $binary=Join-Path $upstream (($(if($withPython){'xloil-wheel312'}else{'xloil-wheel'}))+'/xloil-0.22.1.data/data/share/xloil')
        $files=@('xlOil.xll','xlOil.dll','xlOil_Utils.dll','xlOil_SQL.dll')
        if ($withPython) { $files+=@('xlOil_Python.dll','xlOil_Python312.pyd') }
        foreach ($file in $files) { Copy-Item -LiteralPath (Join-Path $binary $file) -Destination $runtime }
        $settings=@('[Addin]',$(if($withPython){'Plugins=["xlOil_Python", "xlOil_Utils", "xlOil_SQL"]'}else{'Plugins=["xlOil_Utils", "xlOil_SQL"]'}),'LogLevel="trace"','LogPopupLevel="off"','LogFlushLevel="trace"')
        if ($withPython) {
            Copy-Item -LiteralPath (Join-Path $root 'fixture/x3-xloil.py') -Destination (Join-Path $runtime 'xlOil.py')
            $settings+=@('[xlOil_Python]','LoadModules=[]','UseLoaderThread=false','WorkbookModule=""','AddinModule="*.py"','[[xlOil_Python.Environment]]',"PYTHONEXECUTABLE='''$($pythonInfo.executable)'''",'XLOIL_PYTHON_VERSION="312"',"PYTHONPATH='''$upstream/xloil-wheel312;$($pythonInfo.numpy_path)'''")
        }
        $settings | Set-Content -LiteralPath (Join-Path $runtime 'xlOil.ini') -Encoding utf8NoBOM
    }
}
if (-not $NativeOnly -and -not $X3) {
    & dotnet build (Join-Path $root 'fixture/X0.csproj') -c Release
    if ($LASTEXITCODE -ne 0) { throw 'Fixture build failed.' }
    if ($X2) {
        & dotnet build (Join-Path $root 'fixture/X2Hello.csproj') -c Release
        if ($LASTEXITCODE -ne 0) { throw 'Hello fixture build failed.' }
        & dotnet build (Join-Path $root 'fixture/X2Pool.csproj') -c Release
        if ($LASTEXITCODE -ne 0) { throw 'Export pool fixture build failed.' }
        & dotnet build (Join-Path $root 'fixture/X2Identity.csproj') -c Release
        if ($LASTEXITCODE -ne 0) { throw 'Process identity fixture build failed.' }
        $savedPath = $env:PATH
        try {
            $env:PATH = "$([IO.Path]::GetDirectoryName($vswhere));$savedPath"
            & dotnet publish (Join-Path $root 'fixture/X2Aot.csproj') -c Release -r win-x64
            if ($LASTEXITCODE -ne 0) { throw 'Native AOT fixture build failed.' }
        } finally { $env:PATH = $savedPath }
    }
}
