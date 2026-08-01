[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Container })]
    [string]$PackageRoot,

    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$ManifestPath,

    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$TrustPolicyPath,

    [Parameter(Mandatory)]
    [string]$InstalledVersion,

    [Parameter(Mandatory)]
    [ValidateRange(0, [long]::MaxValue)]
    [long]$InstalledReleaseSequence
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'InstallerValidation.psm1') -Force

$trustPolicy = Import-PowerShellDataFile -LiteralPath $TrustPolicyPath
Test-TrustPolicyData -TrustPolicy $trustPolicy | Out-Null

# The update manifest is authenticated before any path or rollback metadata is
# trusted. This script validates only; a caller may launch the bundle after a
# successful result and must not add a bypass switch.
$manifest = Import-TrustedPowerShellDataFile `
    -Path $ManifestPath `
    -TrustPolicy $trustPolicy

Test-UpdatePackageData `
    -Manifest $manifest `
    -PackageRoot $PackageRoot `
    -TrustPolicy $trustPolicy `
    -InstalledVersion $InstalledVersion `
    -InstalledReleaseSequence $InstalledReleaseSequence | Out-Null

Write-Host 'offline update package preflight passed'
