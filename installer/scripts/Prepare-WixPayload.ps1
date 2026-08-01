[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$PayloadRoot,
    [Parameter(Mandatory)] [string]$ManifestPath,
    [Parameter(Mandatory)] [string]$TrustPolicyPath,
    [Parameter(Mandatory)] [string]$OutputRoot,
    [Parameter(Mandatory)] [string]$GeneratedIncludePath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'InstallerValidation.psm1') -Force

$trustPolicy = Import-PowerShellDataFile -LiteralPath $TrustPolicyPath
Test-TrustPolicyData -TrustPolicy $trustPolicy | Out-Null
$manifest = Import-TrustedPowerShellDataFile `
    -Path $ManifestPath `
    -TrustPolicy $trustPolicy
Test-ReleasePayloadData `
    -Manifest $manifest `
    -PayloadRoot $PayloadRoot `
    -TrustPolicy $trustPolicy | Out-Null

# Generated content is confined to installer/**. Validation completes before
# the previous staging tree is touched.
$installerRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$outputFull = [IO.Path]::GetFullPath($OutputRoot)
$includeFull = [IO.Path]::GetFullPath($GeneratedIncludePath)
$allowedPrefix = $installerRoot.TrimEnd('\') + [IO.Path]::DirectorySeparatorChar
foreach ($path in @($outputFull, $includeFull)) {
    if (-not $path.StartsWith($allowedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Generated WiX path '$path' must remain below '$installerRoot'"
    }
}

if (Test-Path -LiteralPath $outputFull) {
    Remove-Item -LiteralPath $outputFull -Recurse -Force
}
New-Item -ItemType Directory -Path $outputFull -Force | Out-Null

foreach ($file in @($manifest.Files)) {
    $relative = ([string]$file.Path).Replace('/', [IO.Path]::DirectorySeparatorChar)
    $source = Join-Path $PayloadRoot $relative
    $destination = Join-Path $outputFull $relative
    New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
    Copy-Item -LiteralPath $source -Destination $destination
}

$installerMetadata = Join-Path $outputFull 'Installer'
New-Item -ItemType Directory -Path $installerMetadata -Force | Out-Null
Copy-Item -LiteralPath $ManifestPath `
    -Destination (Join-Path $installerMetadata 'ReleaseManifest.psd1')
Copy-Item -LiteralPath $TrustPolicyPath `
    -Destination (Join-Path $installerMetadata 'TrustPolicy.psd1')

$includeDirectory = Split-Path -Parent $includeFull
New-Item -ItemType Directory -Path $includeDirectory -Force | Out-Null
$include = @"
<Include>
  <?define ReleaseVersion="$($manifest.Product.Version)" ?>
  <?define ReleaseSequence="$($manifest.Product.ReleaseSequence)" ?>
</Include>
"@
[IO.File]::WriteAllText($includeFull, $include, [Text.UTF8Encoding]::new($false))

Write-Host "validated WiX staging tree prepared at '$outputFull'"
