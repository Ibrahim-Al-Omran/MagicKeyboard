[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

& (Join-Path $PSScriptRoot 'Test-InstallerValidation.ps1')
if (-not $?) {
    exit 1
}
