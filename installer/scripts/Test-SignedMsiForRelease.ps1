[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$MsiPath,
    [Parameter(Mandatory)] [string]$ManifestPath,
    [Parameter(Mandatory)] [string]$TrustPolicyPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'InstallerValidation.psm1') -Force
$trustPolicy = Import-PowerShellDataFile -LiteralPath $TrustPolicyPath
Test-TrustPolicyData -TrustPolicy $trustPolicy | Out-Null
$manifest = Import-TrustedPowerShellDataFile `
    -Path $ManifestPath `
    -TrustPolicy $trustPolicy

& (Join-Path $PSScriptRoot 'Test-SignedMsi.ps1') `
    -MsiPath $MsiPath `
    -TrustPolicyPath $TrustPolicyPath `
    -ExpectedVersion ([string]$manifest.Product.Version)
