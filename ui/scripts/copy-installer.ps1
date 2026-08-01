$ErrorActionPreference = "Stop"

$source = Join-Path $PSScriptRoot "..\src-tauri\target\release\bundle\nsis\Magic Keyboard_0.1.0_x64-setup.exe"
$releases = Join-Path $PSScriptRoot "..\..\releases"
$destination = Join-Path $releases "MagicKeyboard-Setup.exe"

if (-not (Test-Path $source)) {
  throw "Installer not found at $source. Run npm run build:tauri first."
}

New-Item -ItemType Directory -Force -Path $releases | Out-Null
Copy-Item -Force $source $destination

$readme = @"
Magic Keyboard for Windows — installer
======================================

1. Double-click MagicKeyboard-Setup.exe
2. Follow the setup wizard (choose Just me or Everyone)
3. Keep "Launch at Windows startup" enabled in the app settings
4. After install, Magic Keyboard appears in Start Menu and can launch at sign-in

Uninstall from Windows Settings → Apps → Magic Keyboard.
"@
Set-Content -Path (Join-Path $releases "README.txt") -Value $readme -Encoding UTF8

Write-Host "Installer copied to $destination"
