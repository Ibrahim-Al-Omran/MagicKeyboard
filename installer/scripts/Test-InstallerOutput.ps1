[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$MsiPath,

    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$BundlePath,

    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$TrustPolicyPath,

    [Parameter(Mandatory)]
    [string]$ExpectedVersion
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'InstallerValidation.psm1') -Force
$trustPolicy = Import-PowerShellDataFile -LiteralPath $TrustPolicyPath
Test-TrustPolicyData -TrustPolicy $trustPolicy | Out-Null

Test-AuthenticodeFile -Path $MsiPath -SignerGroup payload -TrustPolicy $trustPolicy | Out-Null
Test-AuthenticodeFile -Path $BundlePath -SignerGroup payload -TrustPolicy $trustPolicy | Out-Null
Test-PortableExecutableVersion `
    -Path $BundlePath `
    -ExpectedVersion $ExpectedVersion `
    -Role bundle | Out-Null

$installer = New-Object -ComObject WindowsInstaller.Installer
$database = $null
$view = $null
try {
    $database = $installer.OpenDatabase((Resolve-Path -LiteralPath $MsiPath).Path, 0)
    $view = $database.OpenView(
        "SELECT ``Value`` FROM ``Property`` WHERE ``Property`` = 'ProductVersion'"
    )
    $view.Execute()
    $record = $view.Fetch()
    if ($null -eq $record -or [string]$record.StringData(1) -cne $ExpectedVersion) {
        throw 'signed MSI ProductVersion does not match the release version'
    }
}
finally {
    if ($null -ne $view) {
        $view.Close()
    }
    foreach ($comObject in @($view, $database, $installer)) {
        if ($null -ne $comObject -and [Runtime.InteropServices.Marshal]::IsComObject($comObject)) {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($comObject)
        }
    }
}

Write-Host 'signed installer output validation passed'
