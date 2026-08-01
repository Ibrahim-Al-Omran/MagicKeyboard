[CmdletBinding()]
param(
    [switch]$SkipNative,
    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Debug'
)

$ErrorActionPreference = 'Stop'
$driverRoot = Split-Path -Parent $PSScriptRoot

& (Join-Path $PSScriptRoot 'Test-SafetyInvariants.ps1')

if ($SkipNative) {
    Write-Host 'native mapping tests skipped by request'
    exit 0
}

$cmake = Get-Command cmake -ErrorAction SilentlyContinue
if (-not $cmake) {
    throw 'CMake was not found. Install CMake and a C11 compiler, or use -SkipNative for structural checks only.'
}

$buildDirectory = Join-Path $driverRoot 'out/host'

& $cmake.Source -S $driverRoot -B $buildDirectory
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

& $cmake.Source --build $buildDirectory --config $Configuration
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$ctestPath = Join-Path (Split-Path -Parent $cmake.Source) 'ctest.exe'
if (-not (Test-Path $ctestPath)) {
    $ctest = Get-Command ctest -ErrorAction Stop
    $ctestPath = $ctest.Source
}

& $ctestPath --test-dir $buildDirectory -C $Configuration --output-on-failure
exit $LASTEXITCODE
