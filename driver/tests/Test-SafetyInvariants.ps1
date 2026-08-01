[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$driverRoot = Split-Path -Parent $PSScriptRoot

function Assert-True {
    param(
        [Parameter(Mandatory)]
        [bool]$Condition,

        [Parameter(Mandatory)]
        [string]$Message
    )

    if (-not $Condition) {
        throw "Safety invariant failed: $Message"
    }
}

$inf = Get-Content -Raw (Join-Path $driverRoot 'MagicKeyboardFilter.inf')
$driverSource = Get-Content -Raw (Join-Path $driverRoot 'src/driver.c')
$a1644Source = Get-Content -Raw (Join-Path $driverRoot 'src/a1644.c')
$mappingSource = Get-Content -Raw (Join-Path $driverRoot 'src/mapping.c')
$protocol = Get-Content -Raw (Join-Path $driverRoot 'include/mkfilter_protocol.h')
$project = Get-Content -Raw (Join-Path $driverRoot 'MagicKeyboardFilter.vcxproj')
$notice = Get-Content -Raw (Join-Path (Split-Path -Parent $driverRoot) 'third_party/NOTICE.md')

Assert-True ($inf.Contains('USB\VID_05AC&PID_0267&MI_01')) `
    'INF must bind A1644 USB interface MI_01'
Assert-True (
    $inf.Contains(
        'BTHENUM\{00001124-0000-1000-8000-00805f9b34fb}_VID&0001004c_PID&0267'
    )
) 'INF must bind A1644 Bluetooth HID hardware ID'
Assert-True ($inf -match '(?im)^\s*Class\s*=\s*HIDClass\s*$') `
    'INF must be HIDCLASS (lower filter), not Extension'
Assert-True ($inf -match '(?im)LowerFilters') `
    'INF must register as a HID lower filter'
Assert-True ($inf -notmatch '(?im)HID\\[^\r\n,]*\*') `
    'hardware IDs must not contain wildcards'
Assert-True ($inf -notmatch '(?i)NTarm64') `
    'current INF release boundary must remain x64-only'
Assert-True ($project -match '(?i)DriverType>WDM<') `
    'WDK project must build a WDM lower filter'
Assert-True ($project -notmatch '(?i)ARM64') `
    'current WDK project release boundary must remain x64-only'
Assert-True ($notice -match 'WinAppleKey') `
    'third_party NOTICE must credit WinAppleKey'

Assert-True ($driverSource -match 'IOCTL_INTERNAL_BTH_SUBMIT_BRB') `
    'driver must intercept Bluetooth ACL transfers'
Assert-True ($driverSource -match 'IOCTL_INTERNAL_USB_SUBMIT_URB') `
    'driver must intercept USB interrupt transfers'
Assert-True ($a1644Source -match 'MkFilterProcessA1644Payload') `
    'A1644 rewrite must live in shared host/kernel source'

$hotPathForbidden = @(
    'ZwCreateFile',
    'IoCreateFile',
    'FltCreateFile',
    'Wsk',
    'Tdi',
    'socket('
)

foreach ($token in $hotPathForbidden) {
    Assert-True (-not $a1644Source.Contains($token)) `
        "A1644 rewrite path contains forbidden token $token"
    Assert-True (-not $mappingSource.Contains($token)) `
        "mapping path contains forbidden token $token"
}

Assert-True (
    $protocol -match
    '(?m)^\s*MKFILTER_MAPPING_MAX_ENTRIES\s*=\s*64\s*,?\s*$'
) 'mapping entry limit must remain 64'
Assert-True (
    $mappingSource -match
    'table->Header\.EntryCount\s*>\s*\(uint32_t\)MKFILTER_MAPPING_MAX_ENTRIES'
) 'runtime translation must bound EntryCount before indexing'

Write-Host 'all safety invariant checks passed'
