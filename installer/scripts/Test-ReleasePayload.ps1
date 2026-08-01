[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Container })]
    [string]$PayloadRoot,

    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$ManifestPath,

    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$TrustPolicyPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'InstallerValidation.psm1') -Force

$trustPolicy = Import-PowerShellDataFile -LiteralPath $TrustPolicyPath
Test-TrustPolicyData -TrustPolicy $trustPolicy | Out-Null

# Authenticate before parsing: an unsigned manifest cannot select its own
# payload, versions, hashes, or hardware binding.
$manifest = Import-TrustedPowerShellDataFile `
    -Path $ManifestPath `
    -TrustPolicy $trustPolicy

Test-ReleasePayloadData `
    -Manifest $manifest `
    -PayloadRoot $PayloadRoot `
    -TrustPolicy $trustPolicy | Out-Null

Write-Host 'release payload preflight passed'
