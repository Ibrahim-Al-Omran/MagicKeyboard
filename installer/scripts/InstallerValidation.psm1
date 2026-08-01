Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Throw-Validation {
    param([string]$Message)
    throw "Installer validation failed: $Message"
}

function Assert-MapShape {
    param(
        [Parameter(Mandatory)] [System.Collections.IDictionary]$Map,
        [Parameter(Mandatory)] [string[]]$Required,
        [Parameter(Mandatory)] [string[]]$Allowed,
        [Parameter(Mandatory)] [string]$Name
    )

    foreach ($key in $Required) {
        if (-not $Map.Contains($key)) {
            Throw-Validation "$Name is missing '$key'"
        }
    }

    foreach ($key in $Map.Keys) {
        if ($Allowed -notcontains [string]$key) {
            Throw-Validation "$Name contains unsupported field '$key'"
        }
    }
}

function Assert-ExactSha256 {
    param(
        [Parameter(Mandatory)] [string]$Value,
        [Parameter(Mandatory)] [string]$Name
    )

    if ($Value -notmatch '^[0-9A-Fa-f]{64}$' -or
        $Value -match '^(0{64}|F{64})$') {
        Throw-Validation "$Name must be a non-placeholder 64-digit SHA-256"
    }
}

function ConvertTo-SafeRelativePath {
    param([Parameter(Mandatory)] [string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path) -or
        [IO.Path]::IsPathRooted($Path) -or
        $Path.Contains('\') -or
        $Path -notmatch '^[A-Za-z0-9._-]+(?:/[A-Za-z0-9._-]+)*$') {
        Throw-Validation "payload path '$Path' is not a canonical relative path"
    }

    foreach ($segment in $Path.Split('/')) {
        if ($segment -eq '.' -or $segment -eq '..') {
            Throw-Validation "payload path '$Path' traverses a directory"
        }
    }

    return $Path
}

function Get-CertificateSha256 {
    param([Parameter(Mandatory)] [Security.Cryptography.X509Certificates.X509Certificate2]$Certificate)

    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($Certificate.RawData))).Replace('-', '')
    }
    finally {
        $sha.Dispose()
    }
}

function Test-TrustPolicyData {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [System.Collections.IDictionary]$TrustPolicy)

    $keys = @(
        'SchemaVersion', 'Status', 'ProductId', 'UpgradeCode', 'Architecture',
        'MinimumWindowsBuild', 'AllowedReleaseSignerCertificateSha256',
        'AllowedPayloadSignerCertificateSha256',
        'AllowedDriverSignerCertificateSha256', 'RequireTimestamp',
        'MinimumReleaseSequence'
    )
    Assert-MapShape $TrustPolicy $keys $keys 'trust policy'

    if ([int]$TrustPolicy.SchemaVersion -ne 1) {
        Throw-Validation 'trust policy SchemaVersion must be 1'
    }
    if ([string]$TrustPolicy.Status -cne 'Configured') {
        Throw-Validation 'trust policy Status must be exactly Configured'
    }
    if ([string]$TrustPolicy.ProductId -cne 'MagicKeyboard.Utility' -or
        [string]$TrustPolicy.UpgradeCode -cne
        '{50CB8A9D-BB93-4C83-8FB1-D825C6883C1C}') {
        Throw-Validation 'trust policy product identity does not match the authored MSI'
    }
    if ([string]$TrustPolicy.Architecture -cne 'x64') {
        Throw-Validation 'only the reviewed x64 policy is supported'
    }
    if ([int]$TrustPolicy.MinimumWindowsBuild -lt 22621) {
        Throw-Validation 'trust policy cannot lower the minimum Windows build below 22621'
    }
    if (-not [bool]$TrustPolicy.RequireTimestamp) {
        Throw-Validation 'timestamp verification cannot be disabled'
    }
    if ([int64]$TrustPolicy.MinimumReleaseSequence -lt 1) {
        Throw-Validation 'MinimumReleaseSequence must be positive'
    }

    foreach ($group in @(
        'AllowedReleaseSignerCertificateSha256',
        'AllowedPayloadSignerCertificateSha256',
        'AllowedDriverSignerCertificateSha256'
    )) {
        $values = @($TrustPolicy[$group])
        if ($values.Count -eq 0) {
            Throw-Validation "$group must pin at least one certificate"
        }
        foreach ($value in $values) {
            Assert-ExactSha256 ([string]$value) "trust policy $group entry"
        }
    }

    return $true
}

function Test-AuthenticodeFile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)]
        [ValidateSet('release', 'payload', 'driver')]
        [string]$SignerGroup,
        [Parameter(Mandatory)] [System.Collections.IDictionary]$TrustPolicy
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Throw-Validation "signed file '$Path' does not exist"
    }

    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne [Management.Automation.SignatureStatus]::Valid) {
        Throw-Validation "Authenticode status for '$Path' is $($signature.Status)"
    }
    if ($null -eq $signature.SignerCertificate) {
        Throw-Validation "Authenticode signer certificate is absent for '$Path'"
    }
    if ([bool]$TrustPolicy.RequireTimestamp -and
        $null -eq $signature.TimeStamperCertificate) {
        Throw-Validation "a trusted timestamp is required for '$Path'"
    }

    $policyField = switch ($SignerGroup) {
        'release' { 'AllowedReleaseSignerCertificateSha256' }
        'payload' { 'AllowedPayloadSignerCertificateSha256' }
        'driver' { 'AllowedDriverSignerCertificateSha256' }
    }
    $actual = Get-CertificateSha256 $signature.SignerCertificate
    $allowed = @($TrustPolicy[$policyField]) | ForEach-Object {
        ([string]$_).ToUpperInvariant()
    }
    if ($allowed -notcontains $actual.ToUpperInvariant()) {
        Throw-Validation "untrusted $SignerGroup signer certificate for '$Path'"
    }

    return $true
}

function Import-TrustedPowerShellDataFile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] [System.Collections.IDictionary]$TrustPolicy,
        [scriptblock]$SignatureVerifier
    )

    if ($null -eq $SignatureVerifier) {
        $SignatureVerifier = {
            param($SignedPath, $Group, $Policy)
            Test-AuthenticodeFile -Path $SignedPath -SignerGroup $Group -TrustPolicy $Policy
        }
    }
    & $SignatureVerifier $Path 'release' $TrustPolicy | Out-Null

    try {
        $data = Import-PowerShellDataFile -LiteralPath $Path
    }
    catch {
        Throw-Validation "cannot parse signed data file '$Path': $($_.Exception.Message)"
    }
    if ($data -isnot [System.Collections.IDictionary]) {
        Throw-Validation "signed data file '$Path' must contain one hashtable"
    }
    return $data
}

function Invoke-DriverCatalogVerification {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string]$CatalogPath,
        [Parameter(Mandatory)] [string]$InfPath,
        [Parameter(Mandatory)] [string]$SysPath,
        [Parameter(Mandatory)] [System.Collections.IDictionary]$TrustPolicy,
        [string]$SignToolPath = 'signtool.exe'
    )

    Test-AuthenticodeFile -Path $CatalogPath -SignerGroup driver -TrustPolicy $TrustPolicy | Out-Null

    $command = Get-Command $SignToolPath -ErrorAction SilentlyContinue
    if ($null -eq $command) {
        Throw-Validation 'signtool.exe is required for driver catalog verification'
    }

    $checks = @(
        @('verify', '/kp', '/all', $CatalogPath),
        @('verify', '/kp', '/c', $CatalogPath, $InfPath),
        @('verify', '/kp', '/c', $CatalogPath, $SysPath)
    )
    foreach ($arguments in $checks) {
        & $command.Source @arguments *> $null
        if ($LASTEXITCODE -ne 0) {
            Throw-Validation "signtool catalog verification failed for '$($arguments[-1])'"
        }
    }

    return $true
}

function Test-PortableExecutableVersion {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] [string]$ExpectedVersion,
        [Parameter(Mandatory)] [string]$Role
    )

    $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($Path)
    $actual = @([string]$info.ProductVersion, [string]$info.FileVersion) |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    if ($actual -notcontains $ExpectedVersion) {
        Throw-Validation "$Role version for '$Path' is not exactly '$ExpectedVersion'"
    }
    return $true
}

function Get-PayloadFileMap {
    param(
        [Parameter(Mandatory)] [object[]]$Files,
        [Parameter(Mandatory)] [string]$PayloadRoot
    )

    $result = @{}
    foreach ($file in $Files) {
        if ($file -isnot [System.Collections.IDictionary]) {
            Throw-Validation 'every Files entry must be a hashtable'
        }
        $keys = @('Path', 'Role', 'Sha256', 'Version', 'Signature')
        Assert-MapShape $file $keys $keys 'Files entry'

        $relative = ConvertTo-SafeRelativePath ([string]$file.Path)
        $lookup = $relative.ToUpperInvariant()
        if ($result.ContainsKey($lookup)) {
            Throw-Validation "duplicate payload path '$relative'"
        }
        Assert-ExactSha256 ([string]$file.Sha256) "hash for '$relative'"
        $fullPath = Join-Path $PayloadRoot ($relative.Replace('/', [IO.Path]::DirectorySeparatorChar))
        if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
            Throw-Validation "required payload '$relative' is missing"
        }
        $item = Get-Item -LiteralPath $fullPath -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            Throw-Validation "payload '$relative' must not be a reparse point"
        }
        $result[$lookup] = @{
            Manifest = $file
            FullPath = $item.FullName
            RelativePath = $relative
        }
    }
    return $result
}

function Assert-NoUnlistedPayloadFiles {
    param(
        [Parameter(Mandatory)] [string]$PayloadRoot,
        [Parameter(Mandatory)] [hashtable]$PayloadFileMap
    )

    $root = (Get-Item -LiteralPath $PayloadRoot -Force).FullName.TrimEnd('\', '/')
    foreach ($item in Get-ChildItem -LiteralPath $root -Recurse -File -Force) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            Throw-Validation "payload contains reparse point '$($item.FullName)'"
        }
        $relative = $item.FullName.Substring($root.Length).TrimStart('\', '/').Replace('\', '/')
        if (-not $PayloadFileMap.ContainsKey($relative.ToUpperInvariant())) {
            Throw-Validation "unlisted payload file '$relative' is forbidden"
        }
    }
}

function Get-InfHardwareIds {
    param([Parameter(Mandatory)] [string]$InfText)

    $result = @()
    $matches = [regex]::Matches(
        $InfText,
        '(?im)^\s*[^;=\r\n]+\s*=\s*[^,\r\n]+,\s*([A-Za-z0-9_]+\\[^;\r\n]+)\s*$'
    )
    foreach ($match in $matches) {
        $result += $match.Groups[1].Value.Trim().ToUpperInvariant()
    }
    return @($result | Sort-Object -Unique)
}

function Test-DriverInf {
    param(
        [Parameter(Mandatory)] [string]$InfPath,
        [Parameter(Mandatory)] [System.Collections.IDictionary]$Driver
    )

    $text = Get-Content -LiteralPath $InfPath -Raw
    if ($text -notmatch '(?im)^\s*Class\s*=\s*Extension\s*$') {
        Throw-Validation 'driver INF must remain an Extension INF'
    }
    if ($text -notmatch '(?im)^\s*PnpLockdown\s*=\s*1\s*$') {
        Throw-Validation 'driver INF must enable PnpLockdown'
    }
    if ($text -match '(?im)^\s*Class\s*=\s*Keyboard\s*$' -or
        $text -match '(?im)^\s*UpperFilters\s*=' -or
        $text -match '(?i)\{4D36E96B-E325-11CE-BFC1-08002BE10318\}') {
        Throw-Validation 'class-wide keyboard filtering is forbidden'
    }
    if ($text -match '(?im)^\s*(?:AddReg|DelReg|CopyINF|CoInstallers32|ClassInstall32)\s*=' -or
        $text -match '(?im)^\s*HK(?:LM|CR|CU|U|R)\s*,') {
        Throw-Validation 'driver INF registry, co-installer, and nested-INF mutations are forbidden'
    }
    if ($text -match '(?i)\.NTarm64' -or
        $text -notmatch '(?im)^\[[^\]]+\.NTamd64\.10\.0\.\.\.22621\]\s*$') {
        Throw-Validation 'driver INF models must target Windows x64 build 22621 or newer only'
    }
    if ($text -notmatch '(?im)^\s*AddFilter\s*=' -or
        $text -notmatch '(?im)^\s*FilterPosition\s*=\s*Upper\s*$') {
        Throw-Validation 'driver INF must use a declarative device upper filter'
    }
    if ($text -match '(?i)(?:HID\\[^\r\n,;]*\*|\*PNP[0-9A-F]+|HID_DEVICE_)') {
        Throw-Validation 'wildcard INF hardware IDs are forbidden'
    }

    $ids = @(Get-InfHardwareIds $text)
    if ($ids.Count -eq 0) {
        Throw-Validation 'driver INF has no exact HID hardware IDs'
    }
    foreach ($id in $ids) {
        if ($id -notmatch '^HID\\VID_05AC&PID_[0-9A-F]{4}&MI_[0-9A-F]{2}&COL[0-9A-F]{2}$') {
            Throw-Validation "INF hardware ID '$id' is not exact enough"
        }
        if ($id -match '&PID_(0000|FFFF|FF01|FF02)&' -or
            $id -match '(?i)(PLACEHOLDER|TEMPLATE|EXAMPLE|SYNTHETIC|UNSUPPORTED|TEST)') {
            Throw-Validation "INF hardware ID '$id' is a placeholder"
        }
    }

    $declaredOriginal = @($Driver.HardwareIds | ForEach-Object {
        ([string]$_).ToUpperInvariant()
    })
    $declared = @($declaredOriginal | Sort-Object -Unique)
    if ($declaredOriginal.Count -ne $declared.Count) {
        Throw-Validation 'manifest hardware IDs must not contain duplicates'
    }
    if ($declared.Count -eq 0 -or
        ($ids -join "`n") -cne ($declared -join "`n")) {
        Throw-Validation 'manifest hardware IDs must exactly match all INF model IDs'
    }

    $catalogName = [IO.Path]::GetFileName([string]$Driver.CatalogPath)
    if ($text -notmatch "(?im)^\s*CatalogFile\s*=\s*$([regex]::Escape($catalogName))\s*$") {
        Throw-Validation 'INF CatalogFile does not match the manifest catalog'
    }
    $sysName = [IO.Path]::GetFileName([string]$Driver.SysPath)
    if ($text -notmatch "(?im)^\s*$([regex]::Escape($sysName))\s*=\s*") {
        Throw-Validation 'INF SourceDisksFiles does not name the manifest SYS'
    }

    $driverVersion = [regex]::Match(
        $text,
        '(?im)^\s*DriverVer\s*=\s*[^,]+,\s*([0-9]+(?:\.[0-9]+){3})\s*$'
    )
    if (-not $driverVersion.Success -or
        $driverVersion.Groups[1].Value -cne [string]$Driver.Version) {
        Throw-Validation 'INF DriverVer must exactly match the driver component version'
    }

    return $true
}

function Test-ReleasePayloadData {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [System.Collections.IDictionary]$Manifest,
        [Parameter(Mandatory)] [string]$PayloadRoot,
        [Parameter(Mandatory)] [System.Collections.IDictionary]$TrustPolicy,
        [scriptblock]$SignatureVerifier,
        [scriptblock]$CatalogVerifier,
        [scriptblock]$VersionVerifier
    )

    Test-TrustPolicyData $TrustPolicy | Out-Null
    if (-not (Test-Path -LiteralPath $PayloadRoot -PathType Container)) {
        Throw-Validation "payload root '$PayloadRoot' does not exist"
    }

    $topKeys = @('SchemaVersion', 'Product', 'Components', 'Files')
    Assert-MapShape $Manifest $topKeys $topKeys 'release manifest'
    if ([int]$Manifest.SchemaVersion -ne 1) {
        Throw-Validation 'release manifest SchemaVersion must be 1'
    }

    $productKeys = @(
        'Id', 'Version', 'UpgradeCode', 'Architecture',
        'MinimumWindowsBuild', 'ReleaseSequence'
    )
    Assert-MapShape $Manifest.Product $productKeys $productKeys 'Product'
    if ([string]$Manifest.Product.Id -cne [string]$TrustPolicy.ProductId -or
        [string]$Manifest.Product.UpgradeCode -cne [string]$TrustPolicy.UpgradeCode) {
        Throw-Validation 'manifest product identity does not match the trust policy'
    }
    if ([string]$Manifest.Product.Architecture -cne 'x64' -or
        [string]$Manifest.Product.Architecture -cne [string]$TrustPolicy.Architecture) {
        Throw-Validation 'manifest architecture is not the trusted x64 target'
    }
    if ([int]$Manifest.Product.MinimumWindowsBuild -lt [int]$TrustPolicy.MinimumWindowsBuild) {
        Throw-Validation 'manifest lowers the trusted minimum Windows build'
    }
    if ([int64]$Manifest.Product.ReleaseSequence -lt
        [int64]$TrustPolicy.MinimumReleaseSequence) {
        Throw-Validation 'manifest release sequence is below policy'
    }
    $productVersion = [string]$Manifest.Product.Version
    if ($productVersion -notmatch '^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,5}$') {
        Throw-Validation 'product version must be a three-part MSI-compatible version'
    }
    $versionParts = @($productVersion.Split('.') | ForEach-Object { [int]$_ })
    if ($versionParts[0] -gt 255 -or
        $versionParts[1] -gt 255 -or
        $versionParts[2] -gt 65535) {
        Throw-Validation 'product version exceeds Windows Installer limits'
    }

    $componentKeys = @('Service', 'Ui', 'Driver', 'InstallerActions')
    Assert-MapShape $Manifest.Components $componentKeys $componentKeys 'Components'
    $serviceKeys = @(
        'Version', 'UiProtocolMinimumMajor', 'UiProtocolMaximumMajor',
        'DriverAbiMinimumMajor', 'DriverAbiMaximumMajor'
    )
    Assert-MapShape $Manifest.Components.Service $serviceKeys $serviceKeys 'Service component'
    $uiKeys = @('Version', 'ProtocolMajor')
    Assert-MapShape $Manifest.Components.Ui $uiKeys $uiKeys 'UI component'
    $driverKeys = @(
        'Version', 'AbiMajor', 'InfPath', 'CatalogPath', 'SysPath',
        'HardwareIds', 'BindingApproval'
    )
    Assert-MapShape $Manifest.Components.Driver $driverKeys $driverKeys 'Driver component'
    $actionsKeys = @('Version')
    Assert-MapShape $Manifest.Components.InstallerActions $actionsKeys $actionsKeys 'InstallerActions component'

    if ([string]$Manifest.Components.Service.Version -cne $productVersion -or
        [string]$Manifest.Components.Ui.Version -cne $productVersion -or
        [string]$Manifest.Components.InstallerActions.Version -cne $productVersion) {
        Throw-Validation 'service, UI, and installer-actions versions must equal product version'
    }
    $uiProtocol = [int]$Manifest.Components.Ui.ProtocolMajor
    if ($uiProtocol -lt [int]$Manifest.Components.Service.UiProtocolMinimumMajor -or
        $uiProtocol -gt [int]$Manifest.Components.Service.UiProtocolMaximumMajor) {
        Throw-Validation 'UI protocol major is incompatible with the service'
    }
    $driverAbi = [int]$Manifest.Components.Driver.AbiMajor
    if ($driverAbi -lt [int]$Manifest.Components.Service.DriverAbiMinimumMajor -or
        $driverAbi -gt [int]$Manifest.Components.Service.DriverAbiMaximumMajor) {
        Throw-Validation 'driver ABI major is incompatible with the service'
    }

    $approvalKeys = @('Status', 'EvidencePath', 'EvidenceSha256')
    Assert-MapShape $Manifest.Components.Driver.BindingApproval $approvalKeys $approvalKeys 'BindingApproval'
    if ([string]$Manifest.Components.Driver.BindingApproval.Status -cne 'Approved') {
        Throw-Validation 'device binding review must be explicitly Approved'
    }
    Assert-ExactSha256 ([string]$Manifest.Components.Driver.BindingApproval.EvidenceSha256) 'binding evidence hash'

    $files = @($Manifest.Files)
    $fileMap = Get-PayloadFileMap $files $PayloadRoot
    Assert-NoUnlistedPayloadFiles $PayloadRoot $fileMap

    $requiredRoles = @(
        'service', 'ui', 'driver-inf', 'driver-cat', 'driver-sys',
        'installer-actions', 'binding-evidence'
    )
    $roles = @($files | ForEach-Object { [string]$_.Role })
    foreach ($role in $requiredRoles) {
        if (@($roles | Where-Object { $_ -ceq $role }).Count -ne 1) {
            Throw-Validation "payload must contain exactly one '$role' entry"
        }
    }
    if ($roles.Count -ne $requiredRoles.Count) {
        Throw-Validation 'payload contains an unsupported role'
    }

    $roleMap = @{}
    foreach ($entry in $fileMap.Values) {
        $roleMap[[string]$entry.Manifest.Role] = $entry
        $actualHash = (Get-FileHash -LiteralPath $entry.FullPath -Algorithm SHA256).Hash
        if ($actualHash -cne ([string]$entry.Manifest.Sha256).ToUpperInvariant()) {
            Throw-Validation "SHA-256 mismatch for '$($entry.RelativePath)'"
        }
    }

    $pathExpectations = @{
        'service' = 'Service/MagicKeyboard.Service.exe'
        'ui' = 'UI/MagicKeyboard.Settings.exe'
        'driver-inf' = [string]$Manifest.Components.Driver.InfPath
        'driver-cat' = [string]$Manifest.Components.Driver.CatalogPath
        'driver-sys' = [string]$Manifest.Components.Driver.SysPath
        'installer-actions' = 'Installer/InstallerActions.dll'
        'binding-evidence' = [string]$Manifest.Components.Driver.BindingApproval.EvidencePath
    }
    foreach ($role in $pathExpectations.Keys) {
        if ([string]$roleMap[$role].RelativePath -cne [string]$pathExpectations[$role]) {
            Throw-Validation "$role path does not match its component declaration"
        }
    }
    if (([string]$roleMap['binding-evidence'].Manifest.Sha256).ToUpperInvariant() -cne
        ([string]$Manifest.Components.Driver.BindingApproval.EvidenceSha256).ToUpperInvariant()) {
        Throw-Validation 'binding evidence hashes do not match'
    }

    Test-DriverInf $roleMap['driver-inf'].FullPath $Manifest.Components.Driver | Out-Null

    if ($null -eq $SignatureVerifier) {
        $SignatureVerifier = {
            param($SignedPath, $Group, $Policy)
            Test-AuthenticodeFile -Path $SignedPath -SignerGroup $Group -TrustPolicy $Policy
        }
    }
    if ($null -eq $CatalogVerifier) {
        $CatalogVerifier = {
            param($Catalog, $Inf, $Sys, $Policy)
            Invoke-DriverCatalogVerification `
                -CatalogPath $Catalog -InfPath $Inf -SysPath $Sys -TrustPolicy $Policy
        }
    }
    if ($null -eq $VersionVerifier) {
        $VersionVerifier = {
            param($VersionedPath, $Expected, $Role)
            Test-PortableExecutableVersion `
                -Path $VersionedPath -ExpectedVersion $Expected -Role $Role
        }
    }

    foreach ($role in @('service', 'ui', 'installer-actions')) {
        if ([string]$roleMap[$role].Manifest.Signature -cne 'Authenticode') {
            Throw-Validation "$role must require Authenticode"
        }
        & $SignatureVerifier $roleMap[$role].FullPath 'payload' $TrustPolicy | Out-Null
        & $VersionVerifier $roleMap[$role].FullPath ([string]$roleMap[$role].Manifest.Version) $role | Out-Null
    }
    if ([string]$roleMap['driver-sys'].Manifest.Signature -cne 'AuthenticodeAndCatalogMember' -or
        [string]$roleMap['driver-cat'].Manifest.Signature -cne 'DriverCatalog' -or
        [string]$roleMap['driver-inf'].Manifest.Signature -cne 'CatalogMember') {
        Throw-Validation 'driver signature modes are not strict enough'
    }
    & $SignatureVerifier $roleMap['driver-sys'].FullPath 'driver' $TrustPolicy | Out-Null
    & $VersionVerifier $roleMap['driver-sys'].FullPath ([string]$roleMap['driver-sys'].Manifest.Version) 'driver-sys' | Out-Null
    & $CatalogVerifier `
        $roleMap['driver-cat'].FullPath `
        $roleMap['driver-inf'].FullPath `
        $roleMap['driver-sys'].FullPath `
        $TrustPolicy | Out-Null

    if ([string]$roleMap['driver-inf'].Manifest.Version -cne
        [string]$Manifest.Components.Driver.Version -or
        [string]$roleMap['driver-sys'].Manifest.Version -cne
        [string]$Manifest.Components.Driver.Version) {
        Throw-Validation 'driver payload versions must equal the driver component version'
    }
    if ([string]$roleMap['binding-evidence'].Manifest.Signature -cne 'ManifestHash') {
        Throw-Validation 'binding evidence must be hash-bound by the signed manifest'
    }

    return $true
}

function Test-UpdatePackageData {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [System.Collections.IDictionary]$Manifest,
        [Parameter(Mandatory)] [string]$PackageRoot,
        [Parameter(Mandatory)] [System.Collections.IDictionary]$TrustPolicy,
        [Parameter(Mandatory)] [string]$InstalledVersion,
        [Parameter(Mandatory)] [int64]$InstalledReleaseSequence,
        [scriptblock]$SignatureVerifier,
        [scriptblock]$VersionVerifier
    )

    Test-TrustPolicyData $TrustPolicy | Out-Null
    $keys = @(
        'SchemaVersion', 'ProductId', 'UpgradeCode', 'Version',
        'ReleaseSequence', 'MinimumInstalledVersion', 'Bundle'
    )
    Assert-MapShape $Manifest $keys $keys 'update manifest'
    if ([int]$Manifest.SchemaVersion -ne 1 -or
        [string]$Manifest.ProductId -cne [string]$TrustPolicy.ProductId -or
        [string]$Manifest.UpgradeCode -cne [string]$TrustPolicy.UpgradeCode) {
        Throw-Validation 'update manifest identity is not trusted'
    }

    $bundleKeys = @('Path', 'Sha256', 'Signature')
    Assert-MapShape $Manifest.Bundle $bundleKeys $bundleKeys 'update Bundle'
    if ([string]$Manifest.Bundle.Signature -cne 'Authenticode') {
        Throw-Validation 'update bundle must require Authenticode'
    }
    Assert-ExactSha256 ([string]$Manifest.Bundle.Sha256) 'update bundle hash'

    try {
        $candidateVersion = [version]([string]$Manifest.Version)
        $currentVersion = [version]$InstalledVersion
        $minimumVersion = [version]([string]$Manifest.MinimumInstalledVersion)
    }
    catch {
        Throw-Validation 'update versions must be valid numeric versions'
    }
    if ($candidateVersion -le $currentVersion) {
        Throw-Validation 'update version must be greater than the installed version'
    }
    if ($currentVersion -lt $minimumVersion) {
        Throw-Validation 'installed version is outside this update path'
    }
    if ([int64]$Manifest.ReleaseSequence -le $InstalledReleaseSequence -or
        [int64]$Manifest.ReleaseSequence -lt [int64]$TrustPolicy.MinimumReleaseSequence) {
        Throw-Validation 'update release sequence is a rollback or below policy'
    }

    $relative = ConvertTo-SafeRelativePath ([string]$Manifest.Bundle.Path)
    if ([IO.Path]::GetExtension($relative) -cne '.exe') {
        Throw-Validation 'offline update bundle must be an EXE'
    }
    $bundlePath = Join-Path $PackageRoot $relative
    if (-not (Test-Path -LiteralPath $bundlePath -PathType Leaf)) {
        Throw-Validation "update bundle '$relative' is missing"
    }
    $actualHash = (Get-FileHash -LiteralPath $bundlePath -Algorithm SHA256).Hash
    if ($actualHash -cne ([string]$Manifest.Bundle.Sha256).ToUpperInvariant()) {
        Throw-Validation 'update bundle SHA-256 mismatch'
    }

    if ($null -eq $SignatureVerifier) {
        $SignatureVerifier = {
            param($SignedPath, $Group, $Policy)
            Test-AuthenticodeFile -Path $SignedPath -SignerGroup $Group -TrustPolicy $Policy
        }
    }
    if ($null -eq $VersionVerifier) {
        $VersionVerifier = {
            param($VersionedPath, $Expected, $Role)
            Test-PortableExecutableVersion `
                -Path $VersionedPath -ExpectedVersion $Expected -Role $Role
        }
    }
    & $SignatureVerifier $bundlePath 'payload' $TrustPolicy | Out-Null
    & $VersionVerifier $bundlePath ([string]$Manifest.Version) 'bundle' | Out-Null
    return $true
}

Export-ModuleMember -Function @(
    'Import-TrustedPowerShellDataFile',
    'Invoke-DriverCatalogVerification',
    'Test-AuthenticodeFile',
    'Test-PortableExecutableVersion',
    'Test-ReleasePayloadData',
    'Test-TrustPolicyData',
    'Test-UpdatePackageData'
)
