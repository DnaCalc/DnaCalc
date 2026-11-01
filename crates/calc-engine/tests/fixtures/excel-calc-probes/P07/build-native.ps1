$ErrorActionPreference='Stop'
& (Join-Path $PSScriptRoot '../P03/build-native.ps1') -Source (Join-Path $PSScriptRoot 'native-async.cpp') -Output (Join-Path $PSScriptRoot 'native-async.xll')
