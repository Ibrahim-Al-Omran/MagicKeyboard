#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Removes MagicKeyboardFilter so the keyboard falls back to the inbox HID driver.

.PARAMETER DisableTestSigning
    Also run bcdedit /set testsigning off (takes effect after reboot).
#>
[CmdletBinding()]
param(
    [switch]$DisableTestSigning
)

$ErrorActionPreference = 'Stop'

$current = @{}
foreach ($line in (pnputil /enum-drivers)) {
    if ($line -match '^\s*Published Name:\s*(\S+)') { $current = @{ Published = $Matches[1] } }
    elseif ($line -match '^\s*Original Name:\s*(\S+)') {
        if ($Matches[1] -eq 'magickeyboardfilter.inf') {
            pnputil /delete-driver $current.Published /uninstall /force | Out-Null
            Write-Host "Removed $($current.Published)"
        }
    }
}

pnputil /scan-devices | Out-Null
sc.exe delete MagicKeyboardFilter 2>$null | Out-Null

if ($DisableTestSigning) {
    bcdedit /set testsigning off | Out-Null
    Write-Host 'Test signing will be off after reboot.'
}
Write-Host 'Done. Reconnect the keyboard (or reboot). Reinstall Magic Utilities if you want its driver back.'
