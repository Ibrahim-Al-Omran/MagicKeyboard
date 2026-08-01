[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$MsiPath,

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

$installer = New-Object -ComObject WindowsInstaller.Installer
$database = $null
try {
    $database = $installer.OpenDatabase((Resolve-Path -LiteralPath $MsiPath).Path, 0)
    $expected = @{
        ProductVersion = $ExpectedVersion
        UpgradeCode = [string]$trustPolicy.UpgradeCode
    }
    foreach ($property in $expected.Keys) {
        $view = $null
        try {
            $query = "SELECT ``Value`` FROM ``Property`` WHERE ``Property`` = '$property'"
            $view = $database.OpenView($query)
            $view.Execute()
            $record = $view.Fetch()
            if ($null -eq $record -or
                [string]$record.StringData(1) -cne [string]$expected[$property]) {
                throw "signed MSI $property does not match the trusted release"
            }
        }
        finally {
            if ($null -ne $view) {
                $view.Close()
                [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($view)
            }
        }
    }
}
finally {
    foreach ($comObject in @($database, $installer)) {
        if ($null -ne $comObject -and [Runtime.InteropServices.Marshal]::IsComObject($comObject)) {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($comObject)
        }
    }
}

Write-Host 'signed MSI validation passed'
