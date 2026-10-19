#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Test-signs and installs MagicKeyboardFilter on this machine's A1644 keyboard.

.DESCRIPTION
    1. Packages x64\Release\MagicKeyboardFilter.{sys,inf} with a signed catalog.
    2. Trusts the WDK test certificate (LocalMachine Root + TrustedPublisher).
    3. Optionally removes the Magic Utilities keyboard driver, which otherwise
       owns the same LowerFilters slot.
    4. Installs the filter and restarts the keyboard device.

    Requires test signing to be enabled (bcdedit /set testsigning on + reboot).
    Keep a second keyboard connected. Undo with Uninstall-TestFilter.ps1.

.PARAMETER RemoveMagicUtilitiesDriver
    Remove Magic Utilities' magickeyboard.inf from the driver store first.
    Quit the Magic Utilities app beforehand so its service does not reinstall it.

.PARAMETER EnableTestSigning
    Run bcdedit /set testsigning on and stop; reboot, then run this script again.
#>
[CmdletBinding()]
param(
    [switch]$RemoveMagicUtilitiesDriver,
    [switch]$EnableTestSigning
)

$ErrorActionPreference = 'Stop'
$driverDir = Split-Path $PSScriptRoot -Parent
$release = Join-Path $driverDir 'x64\Release'
$package = Join-Path $release 'package'
$keyboardIdPattern = '^(BTHENUM\\\{00001124-0000-1000-8000-00805F9B34FB\}_VID&0001004C_PID&0267|USB\\VID_05AC&PID_0267&MI_01)\\'

function Get-KitTool([string]$name, [string]$arch) {
    $tool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter $name -ErrorAction SilentlyContinue |
        Where-Object { $_.Directory.Name -eq $arch } |
        Sort-Object FullName -Descending |
        Select-Object -First 1
    if (-not $tool) { throw "$name ($arch) not found in the Windows Kits. Install the WDK." }
    $tool.FullName
}

function Get-OemInf([string]$originalName) {
    $current = @{}
    $found = @()
    foreach ($line in (pnputil /enum-drivers)) {
        if ($line -match '^\s*Published Name:\s*(\S+)') { $current = @{ Published = $Matches[1] } }
        elseif ($line -match '^\s*Original Name:\s*(\S+)') { $current.Original = $Matches[1] }
        if ($current.Original -eq $originalName -and $current.Published -and $found -notcontains $current.Published) {
            $found += $current.Published
        }
    }
    $found
}

# --- 0. Test signing --------------------------------------------------------
$testSigning = (bcdedit /enum '{current}' | Select-String '^\s*testsigning\s+Yes') -ne $null
if ($EnableTestSigning) {
    if ($testSigning) { Write-Host 'Test signing is already on.'; return }
    bcdedit /set testsigning on
    if ($LASTEXITCODE -ne 0) {
        throw 'bcdedit refused. Secure Boot is probably on: disable it in UEFI/BIOS setup, then retry.'
    }
    Write-Host 'Test signing enabled. Reboot, then run this script again without -EnableTestSigning.'
    return
}
if (-not $testSigning) {
    throw 'Test signing is off, so Windows will refuse the driver. Run with -EnableTestSigning first.'
}

# --- 1. Package -------------------------------------------------------------
$sys = Join-Path $release 'MagicKeyboardFilter.sys'
$inf = Join-Path $release 'MagicKeyboardFilter.inf'
$cer = Join-Path $release 'MagicKeyboardFilter.cer'
foreach ($f in $sys, $inf, $cer) {
    if (-not (Test-Path $f)) { throw "Missing $f. Build MagicKeyboardFilter.vcxproj (Release|x64) first." }
}

$signer = (Get-AuthenticodeSignature $sys).SignerCertificate
if (-not $signer) { throw "$sys is not signed. Rebuild so the WDK test-signs it." }
$signingCert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Thumbprint -eq $signer.Thumbprint -and $_.HasPrivateKey }
if (-not $signingCert) { throw "Private key for $($signer.Subject) not found in CurrentUser\My." }

Remove-Item $package -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $package | Out-Null
Copy-Item $sys, $inf $package

& (Get-KitTool 'Inf2Cat.exe' 'x86') /driver:$package /os:10_X64 | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Inf2Cat failed.' }
& (Get-KitTool 'signtool.exe' 'x64') sign /fd sha256 /sha1 $signer.Thumbprint (Join-Path $package 'MagicKeyboardFilter.cat') | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'signtool failed to sign the catalog.' }
Write-Host "Packaged and signed: $package"

# --- 2. Trust the test certificate -----------------------------------------
foreach ($store in 'Root', 'TrustedPublisher') {
    Import-Certificate -FilePath $cer -CertStoreLocation "Cert:\LocalMachine\$store" | Out-Null
}
Write-Host "Trusted test certificate $($signer.Thumbprint)"

# --- 3. Magic Utilities driver ---------------------------------------------
$muInfs = Get-OemInf 'magickeyboard.inf'
if ($muInfs) {
    if (-not $RemoveMagicUtilitiesDriver) {
        throw "Magic Utilities' keyboard driver ($($muInfs -join ', ')) is installed and owns the keyboard's filter slot. Quit Magic Utilities and re-run with -RemoveMagicUtilitiesDriver."
    }
    foreach ($oem in $muInfs) {
        pnputil /delete-driver $oem /uninstall /force | Out-Null
        Write-Host "Removed Magic Utilities driver $oem"
    }
}

# --- 4. Install and restart -------------------------------------------------
pnputil /add-driver (Join-Path $package 'MagicKeyboardFilter.inf') /install
if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne 3010) { throw "pnputil /add-driver failed ($LASTEXITCODE)." }

$keyboards = Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -match $keyboardIdPattern }
if (-not $keyboards) {
    Write-Warning 'No A1644 keyboard is connected. Connect it; Windows will apply the filter on arrival.'
    return
}
foreach ($kb in $keyboards) {
    pnputil /restart-device "$($kb.InstanceId)" | Out-Null
    $filters = (Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Enum\$($kb.InstanceId)").LowerFilters
    $state = if ($filters -contains 'MagicKeyboardFilter') { 'OK' } else { "NOT ATTACHED (LowerFilters: $($filters -join ','))" }
    Write-Host "$($kb.FriendlyName): $state"
}
Write-Host 'Test: open Notepad and press Eject; it should delete. If the keyboard stops responding, run Uninstall-TestFilter.ps1 from another keyboard.'
