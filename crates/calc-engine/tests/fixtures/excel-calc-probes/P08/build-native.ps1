$ErrorActionPreference='Stop'
& (Join-Path $PSScriptRoot '../P03/build-native.ps1') -Source (Join-Path $PSScriptRoot 'native-order.cpp') -Output (Join-Path $PSScriptRoot 'native-order.xll')
