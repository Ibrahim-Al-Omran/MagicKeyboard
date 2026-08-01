[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (
    Join-Path (Split-Path -Parent $PSScriptRoot) 'scripts\InstallerValidation.psm1'
) -Force

$script:passed = 0
$script:failed = 0

function Assert-Throws {
    param(
        [Parameter(Mandatory)] [scriptblock]$Action,
        [Parameter(Mandatory)] [string]$Pattern
    )

    try {
        & $Action
    }
    catch {
        if ($_.Exception.Message -notmatch $Pattern) {
            throw "Expected error /$Pattern/, got: $($_.Exception.Message)"
        }
        return
    }
    throw "Expected error /$Pattern/, but no error was thrown"
}

function Invoke-Test {
    param(
        [Parameter(Mandatory)] [string]$Name,
        [Parameter(Mandatory)] [scriptblock]$Action
    )

    try {
        & $Action
        $script:passed++
        Write-Host "PASS $Name"
    }
    catch {
        $script:failed++
        Write-Host "FAIL $Name`n  $($_.Exception.Message)"
    }
}

function Get-RoleEntry {
    param(
        [Parameter(Mandatory)] [System.Collections.IDictionary]$Manifest,
        [Parameter(Mandatory)] [string]$Role
    )
    return @($Manifest.Files | Where-Object { [string]$_.Role -ceq $Role })[0]
}

function New-ValidationFixture {
    $root = Join-Path ([IO.Path]::GetTempPath()) (
        'mk-installer-fixture-' + [guid]::NewGuid().ToString('N')
    )
    New-Item -ItemType Directory -Path $root | Out-Null

    $paths = @(
        'Service/MagicKeyboard.Service.exe',
        'UI/MagicKeyboard.Settings.exe',
        'Driver/MagicKeyboardFilter.inf',
        'Driver/MagicKeyboardFilter.cat',
        'Driver/MagicKeyboardFilter.sys',
        'Installer/InstallerActions.dll',
        'Evidence/device-binding-review.json'
    )
    foreach ($relative in $paths) {
        $full = Join-Path $root $relative
        New-Item -ItemType Directory -Path (Split-Path -Parent $full) -Force | Out-Null
        [IO.File]::WriteAllText($full, "fixture:$relative")
    }

    # Synthetic test-only ID; it is not a claim about an Apple product.
    $hardwareId = 'HID\VID_05AC&PID_' + 'A17E' + '&MI_7D&COL7C'
    $inf = @"
[Version]
Signature = "`$WINDOWS NT`$"
Class = Extension
ClassGuid = {E2F84CE7-8EFA-411C-AA69-97454CA4CB57}
DriverVer = 07/31/2026,0.1.0.0
CatalogFile = MagicKeyboardFilter.cat
PnpLockdown = 1

[SourceDisksFiles]
MagicKeyboardFilter.sys = 1

[SyntheticModels.NTamd64.10.0...22621]
%SyntheticFixtureDevice% = Fixture.Install, $hardwareId

[Fixture.Install.NT.Filters]
AddFilter = MagicKeyboardFilter,, Fixture.UpperFilter

[Fixture.UpperFilter]
FilterPosition = Upper
"@
    [IO.File]::WriteAllText((Join-Path $root 'Driver/MagicKeyboardFilter.inf'), $inf)
    [IO.File]::WriteAllText(
        (Join-Path $root 'Evidence/device-binding-review.json'),
        '{"fixtureOnly":true,"status":"approved"}'
    )

    $roles = [ordered]@{
        service = @('Service/MagicKeyboard.Service.exe', '0.1.0', 'Authenticode')
        ui = @('UI/MagicKeyboard.Settings.exe', '0.1.0', 'Authenticode')
        'driver-inf' = @('Driver/MagicKeyboardFilter.inf', '0.1.0.0', 'CatalogMember')
        'driver-cat' = @('Driver/MagicKeyboardFilter.cat', '', 'DriverCatalog')
        'driver-sys' = @(
            'Driver/MagicKeyboardFilter.sys',
            '0.1.0.0',
            'AuthenticodeAndCatalogMember'
        )
        'installer-actions' = @(
            'Installer/InstallerActions.dll',
            '0.1.0',
            'Authenticode'
        )
        'binding-evidence' = @(
            'Evidence/device-binding-review.json',
            '',
            'ManifestHash'
        )
    }

    $files = @()
    foreach ($role in $roles.Keys) {
        $definition = $roles[$role]
        $relative = [string]$definition[0]
        $files += @{
            Path = $relative
            Role = [string]$role
            Sha256 = (Get-FileHash -LiteralPath (Join-Path $root $relative) -Algorithm SHA256).Hash
            Version = [string]$definition[1]
            Signature = [string]$definition[2]
        }
    }

    $evidenceHash = [string](Get-RoleEntry @{ Files = $files } 'binding-evidence').Sha256
    $manifest = @{
        SchemaVersion = 1
        Product = @{
            Id = 'MagicKeyboard.Utility'
            Version = '0.1.0'
            UpgradeCode = '{50CB8A9D-BB93-4C83-8FB1-D825C6883C1C}'
            Architecture = 'x64'
            MinimumWindowsBuild = 22621
            ReleaseSequence = 2
        }
        Components = @{
            Service = @{
                Version = '0.1.0'
                UiProtocolMinimumMajor = 1
                UiProtocolMaximumMajor = 1
                DriverAbiMinimumMajor = 1
                DriverAbiMaximumMajor = 1
            }
            Ui = @{
                Version = '0.1.0'
                ProtocolMajor = 1
            }
            Driver = @{
                Version = '0.1.0.0'
                AbiMajor = 1
                InfPath = 'Driver/MagicKeyboardFilter.inf'
                CatalogPath = 'Driver/MagicKeyboardFilter.cat'
                SysPath = 'Driver/MagicKeyboardFilter.sys'
                HardwareIds = @($hardwareId)
                BindingApproval = @{
                    Status = 'Approved'
                    EvidencePath = 'Evidence/device-binding-review.json'
                    EvidenceSha256 = $evidenceHash
                }
            }
            InstallerActions = @{
                Version = '0.1.0'
            }
        }
        Files = $files
    }

    $trust = @{
        SchemaVersion = 1
        Status = 'Configured'
        ProductId = 'MagicKeyboard.Utility'
        UpgradeCode = '{50CB8A9D-BB93-4C83-8FB1-D825C6883C1C}'
        Architecture = 'x64'
        MinimumWindowsBuild = 22621
        AllowedReleaseSignerCertificateSha256 = @('11' * 32)
        AllowedPayloadSignerCertificateSha256 = @('22' * 32)
        AllowedDriverSignerCertificateSha256 = @('33' * 32)
        RequireTimestamp = $true
        MinimumReleaseSequence = 1
    }

    return [pscustomobject]@{
        Root = $root
        Manifest = $manifest
        Trust = $trust
        SignatureVerifier = { return $true }
        CatalogVerifier = { return $true }
        VersionVerifier = { return $true }
    }
}

function Remove-ValidationFixture {
    param([Parameter(Mandatory)]$Fixture)
    Remove-Item -LiteralPath $Fixture.Root -Recurse -Force -ErrorAction SilentlyContinue
}

function Invoke-ValidReleaseCheck {
    param([Parameter(Mandatory)]$Fixture)
    Test-ReleasePayloadData `
        -Manifest $Fixture.Manifest `
        -PayloadRoot $Fixture.Root `
        -TrustPolicy $Fixture.Trust `
        -SignatureVerifier $Fixture.SignatureVerifier `
        -CatalogVerifier $Fixture.CatalogVerifier `
        -VersionVerifier $Fixture.VersionVerifier | Out-Null
}

Invoke-Test 'accepts a coherent synthetic fixture' {
    $fixture = New-ValidationFixture
    try {
        Invoke-ValidReleaseCheck $fixture
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects a file hash mismatch' {
    $fixture = New-ValidationFixture
    try {
        Add-Content -LiteralPath (Join-Path $fixture.Root 'Service/MagicKeyboard.Service.exe') -Value 'tampered'
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'SHA-256 mismatch'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects an untrusted payload signature' {
    $fixture = New-ValidationFixture
    try {
        $fixture.SignatureVerifier = { throw 'fixture signature rejected' }
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'signature rejected'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects failed driver catalog membership' {
    $fixture = New-ValidationFixture
    try {
        $fixture.CatalogVerifier = { throw 'catalog member rejected' }
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'catalog member rejected'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects a placeholder INF hardware ID' {
    $fixture = New-ValidationFixture
    try {
        $placeholder = 'HID\VID_05AC&PID_' + 'FF01' + '&MI_00&COL01'
        $infPath = Join-Path $fixture.Root 'Driver/MagicKeyboardFilter.inf'
        $text = (Get-Content -LiteralPath $infPath -Raw).Replace(
            [string]$fixture.Manifest.Components.Driver.HardwareIds[0],
            $placeholder
        )
        [IO.File]::WriteAllText($infPath, $text)
        $fixture.Manifest.Components.Driver.HardwareIds = @($placeholder)
        (Get-RoleEntry $fixture.Manifest 'driver-inf').Sha256 =
            (Get-FileHash -LiteralPath $infPath -Algorithm SHA256).Hash
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'placeholder'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects class-wide filter registration' {
    $fixture = New-ValidationFixture
    try {
        $infPath = Join-Path $fixture.Root 'Driver/MagicKeyboardFilter.inf'
        Add-Content -LiteralPath $infPath -Value "`nUpperFilters = MagicKeyboardFilter"
        (Get-RoleEntry $fixture.Manifest 'driver-inf').Sha256 =
            (Get-FileHash -LiteralPath $infPath -Algorithm SHA256).Hash
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'class-wide'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects an additional non-device-specific model ID' {
    $fixture = New-ValidationFixture
    try {
        $infPath = Join-Path $fixture.Root 'Driver/MagicKeyboardFilter.inf'
        $broadId = 'USB\Class_' + '03'
        Add-Content `
            -LiteralPath $infPath `
            -Value "`n%BroadFixtureDevice% = Fixture.Install, $broadId"
        (Get-RoleEntry $fixture.Manifest 'driver-inf').Sha256 =
            (Get-FileHash -LiteralPath $infPath -Algorithm SHA256).Hash
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'not exact enough'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects incompatible UI protocol versions' {
    $fixture = New-ValidationFixture
    try {
        $fixture.Manifest.Components.Ui.ProtocolMajor = 2
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'UI protocol major'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects unapproved binding evidence' {
    $fixture = New-ValidationFixture
    try {
        $fixture.Manifest.Components.Driver.BindingApproval.Status = 'Unapproved'
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'explicitly Approved'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects unlisted payload files' {
    $fixture = New-ValidationFixture
    try {
        [IO.File]::WriteAllText((Join-Path $fixture.Root 'extra.exe'), 'not declared')
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'unlisted payload'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects path traversal' {
    $fixture = New-ValidationFixture
    try {
        (Get-RoleEntry $fixture.Manifest 'service').Path = '../service.exe'
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'canonical relative path|traverses'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects placeholder hashes' {
    $fixture = New-ValidationFixture
    try {
        (Get-RoleEntry $fixture.Manifest 'service').Sha256 = '0' * 64
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'non-placeholder'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects an unconfigured trust policy' {
    $fixture = New-ValidationFixture
    try {
        $fixture.Trust.Status = 'Unconfigured'
        Assert-Throws { Invoke-ValidReleaseCheck $fixture } 'exactly Configured'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'accepts a forward offline update' {
    $fixture = New-ValidationFixture
    try {
        $bundlePath = Join-Path $fixture.Root 'candidate.exe'
        [IO.File]::WriteAllText($bundlePath, 'signed-bundle-fixture')
        $update = @{
            SchemaVersion = 1
            ProductId = 'MagicKeyboard.Utility'
            UpgradeCode = '{50CB8A9D-BB93-4C83-8FB1-D825C6883C1C}'
            Version = '0.2.0'
            ReleaseSequence = 3
            MinimumInstalledVersion = '0.1.0'
            Bundle = @{
                Path = 'candidate.exe'
                Sha256 = (Get-FileHash -LiteralPath $bundlePath -Algorithm SHA256).Hash
                Signature = 'Authenticode'
            }
        }
        Test-UpdatePackageData `
            -Manifest $update `
            -PackageRoot $fixture.Root `
            -TrustPolicy $fixture.Trust `
            -InstalledVersion '0.1.0' `
            -InstalledReleaseSequence 2 `
            -SignatureVerifier { return $true } `
            -VersionVerifier { return $true } | Out-Null
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Invoke-Test 'rejects an update rollback sequence' {
    $fixture = New-ValidationFixture
    try {
        $bundlePath = Join-Path $fixture.Root 'candidate.exe'
        [IO.File]::WriteAllText($bundlePath, 'signed-bundle-fixture')
        $update = @{
            SchemaVersion = 1
            ProductId = 'MagicKeyboard.Utility'
            UpgradeCode = '{50CB8A9D-BB93-4C83-8FB1-D825C6883C1C}'
            Version = '0.2.0'
            ReleaseSequence = 2
            MinimumInstalledVersion = '0.1.0'
            Bundle = @{
                Path = 'candidate.exe'
                Sha256 = (Get-FileHash -LiteralPath $bundlePath -Algorithm SHA256).Hash
                Signature = 'Authenticode'
            }
        }
        Assert-Throws {
            Test-UpdatePackageData `
                -Manifest $update `
                -PackageRoot $fixture.Root `
                -TrustPolicy $fixture.Trust `
                -InstalledVersion '0.1.0' `
                -InstalledReleaseSequence 2 `
                -SignatureVerifier { return $true } `
                -VersionVerifier { return $true } | Out-Null
        } 'rollback'
    }
    finally {
        Remove-ValidationFixture $fixture
    }
}

Write-Host "$script:passed passed; $script:failed failed"
if ($script:failed -ne 0) {
    exit 1
}
