[CmdletBinding()]
param(
    [switch]$Strict
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$results = New-Object 'System.Collections.Generic.List[object]'
$missingRequiredToolchains = @{}

function Add-Result {
    param(
        [Parameter(Mandatory)]
        [string]$Name,

        [Parameter(Mandatory)]
        [ValidateSet('PASS', 'FAILED', 'SKIPPED')]
        [string]$Status,

        [string]$Detail = ''
    )

    $results.Add([pscustomobject]@{
        Name = $Name
        Status = $Status
        Detail = $Detail
    }) | Out-Null

    $color = switch ($Status) {
        'PASS' { 'Green' }
        'FAILED' { 'Red' }
        default { 'Yellow' }
    }

    $suffix = if ($Detail) { ": $Detail" } else { '' }
    Write-Host ("[{0}] {1}{2}" -f $Status, $Name, $suffix) -ForegroundColor $color
}

function Add-ComponentSkip {
    param(
        [Parameter(Mandatory)]
        [string]$Name,

        [Parameter(Mandatory)]
        [string]$Reason
    )

    Add-Result -Name $Name -Status 'SKIPPED' -Detail $Reason
}

function Add-ToolchainSkip {
    param(
        [Parameter(Mandatory)]
        [string]$Name,

        [Parameter(Mandatory)]
        [string]$Toolchain
    )

    $missingRequiredToolchains[$Toolchain] = $true
    Add-Result -Name $Name -Status 'SKIPPED' -Detail "required toolchain unavailable: $Toolchain"
}

function Get-Tool {
    param(
        [Parameter(Mandatory)]
        [string[]]$Names
    )

    foreach ($name in $Names) {
        $tool = Get-Command -Name $name -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($tool) {
            return $tool
        }
    }

    return $null
}

function Invoke-Program {
    param(
        [Parameter(Mandatory)]
        [string]$Command,

        [string[]]$Arguments = @(),

        [Parameter(Mandatory)]
        [string]$WorkingDirectory
    )

    Push-Location $WorkingDirectory
    try {
        & $Command @Arguments | Out-Host
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            throw "Command exited with code ${exitCode}: $Command $($Arguments -join ' ')"
        }
    }
    finally {
        Pop-Location
    }
}

function Invoke-Step {
    param(
        [Parameter(Mandatory)]
        [string]$Name,

        [Parameter(Mandatory)]
        [scriptblock]$Action
    )

    Write-Host "`n==> $Name"
    try {
        & $Action
        Add-Result -Name $Name -Status 'PASS'
        return $true
    }
    catch {
        Add-Result -Name $Name -Status 'FAILED' -Detail $_.Exception.Message
        return $false
    }
}

if ($PSVersionTable.PSEdition -eq 'Core') {
    $powerShellExecutable = Join-Path $PSHOME 'pwsh.exe'
}
else {
    $powerShellExecutable = Join-Path $PSHOME 'powershell.exe'
}

function Invoke-PowerShellScript {
    param(
        [Parameter(Mandatory)]
        [string]$Path,

        [string[]]$ScriptArguments = @()
    )

    $arguments = @(
        '-NoLogo',
        '-NoProfile',
        '-NonInteractive',
        '-ExecutionPolicy',
        'Bypass',
        '-File',
        $Path
    ) + $ScriptArguments

    Invoke-Program -Command $powerShellExecutable `
        -Arguments $arguments `
        -WorkingDirectory $repoRoot
}

function Test-CCompilerAvailable {
    if (Get-Tool -Names @('cl.exe', 'clang.exe', 'gcc.exe')) {
        return $true
    }

    if (${env:ProgramFiles(x86)}) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} `
            'Microsoft Visual Studio\Installer\vswhere.exe'
        if (Test-Path $vswhere) {
            $installation = & $vswhere -latest -products '*' `
                -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
                -property installationPath 2>$null
            if (($LASTEXITCODE -eq 0) -and $installation) {
                return $true
            }
        }
    }

    return $false
}

function Test-RepositoryStaticSyntax {
    $jsonFiles = @(
        'ui/package.json',
        'ui/tsconfig.json',
        'ui/src-tauri/tauri.conf.json',
        'ui/src-tauri/capabilities/settings-window.json'
    )
    foreach ($relativePath in $jsonFiles) {
        Get-Content -LiteralPath (Join-Path $repoRoot $relativePath) -Raw |
            ConvertFrom-Json | Out-Null
    }

    $xmlFiles = @(
        'driver/MagicKeyboardFilter.vcxproj',
        'driver/MagicKeyboardFilter.vcxproj.filters',
        'installer/wix/Product.wixproj',
        'installer/wix/Bundle.wixproj',
        'installer/wix/Product.wxs',
        'installer/wix/Bundle.wxs'
    )
    foreach ($relativePath in $xmlFiles) {
        [xml](Get-Content -LiteralPath (Join-Path $repoRoot $relativePath) -Raw) |
            Out-Null
    }

    $parseFailures = New-Object 'System.Collections.Generic.List[string]'
    foreach ($relativeRoot in @('scripts', 'driver', 'installer')) {
        $files = Get-ChildItem -LiteralPath (Join-Path $repoRoot $relativeRoot) `
            -Recurse -File |
            Where-Object { $_.Extension -in @('.ps1', '.psm1') }
        foreach ($file in $files) {
            $tokens = $null
            $parseErrors = $null
            [void][System.Management.Automation.Language.Parser]::ParseFile(
                $file.FullName,
                [ref]$tokens,
                [ref]$parseErrors
            )
            foreach ($parseError in $parseErrors) {
                $parseFailures.Add(
                    "$($file.FullName):$($parseError.Extent.StartLineNumber): " +
                    $parseError.Message
                )
            }
        }
    }

    if ($parseFailures.Count -gt 0) {
        throw ($parseFailures -join [Environment]::NewLine)
    }
}

Write-Host "MagicKeyboard repository tests"
Write-Host "Repository: $repoRoot"
Write-Host "Mode: $(if ($Strict) { 'strict' } else { 'developer' })"

Invoke-Step -Name 'Repository static syntax' -Action {
    Test-RepositoryStaticSyntax
} | Out-Null

$cargo = Get-Tool -Names @('cargo.exe', 'cargo')
$workspaceManifest = Join-Path $repoRoot 'Cargo.toml'
if (Test-Path $workspaceManifest) {
    if ($cargo) {
        Invoke-Step -Name 'Rust workspace tests' -Action {
            Invoke-Program -Command $cargo.Source `
                -Arguments @('test', '--workspace', '--all-targets') `
                -WorkingDirectory $repoRoot
        } | Out-Null
    }
    else {
        Add-ToolchainSkip -Name 'Rust workspace tests' -Toolchain 'Rust/Cargo'
    }
}
else {
    Add-ComponentSkip -Name 'Rust workspace tests' `
        -Reason 'workspace Cargo.toml is not present'
}

$hidProbeManifest = Join-Path $repoRoot 'tools/hid-probe/Cargo.toml'
if (Test-Path $hidProbeManifest) {
    if ($cargo) {
        Invoke-Step -Name 'Standalone hid-probe tests' -Action {
            Invoke-Program -Command $cargo.Source `
                -Arguments @(
                    'test',
                    '--manifest-path',
                    $hidProbeManifest,
                    '--all-targets'
                ) `
                -WorkingDirectory (Split-Path -Parent $hidProbeManifest)
        } | Out-Null
    }
    else {
        Add-ToolchainSkip -Name 'Standalone hid-probe tests' `
            -Toolchain 'Rust/Cargo'
    }
}
else {
    Add-ComponentSkip -Name 'Standalone hid-probe tests' `
        -Reason 'standalone manifest is not present'
}

$tauriFrontendReady = $false
$uiRoot = Join-Path $repoRoot 'ui'
$uiManifest = Join-Path $uiRoot 'package.json'
if (Test-Path $uiManifest) {
    $node = Get-Tool -Names @('node.exe', 'node')
    $npm = Get-Tool -Names @('npm.cmd', 'npm.exe', 'npm')

    if ($node -and $npm) {
        $dependenciesReady = Invoke-Step -Name 'UI dependency install' -Action {
            Invoke-Program -Command $npm.Source `
                -Arguments @('ci') `
                -WorkingDirectory $uiRoot
        }

        if ($dependenciesReady) {
            Invoke-Step -Name 'UI tests' -Action {
                Invoke-Program -Command $npm.Source `
                    -Arguments @('test') `
                    -WorkingDirectory $uiRoot
            } | Out-Null

            Invoke-Step -Name 'UI production build' -Action {
                Invoke-Program -Command $npm.Source `
                    -Arguments @('run', 'build') `
                    -WorkingDirectory $uiRoot
            } | Out-Null

            $tauriFrontendReady = Invoke-Step -Name 'UI Tauri frontend build' -Action {
                Invoke-Program -Command $npm.Source `
                    -Arguments @('run', 'build:tauri:frontend') `
                    -WorkingDirectory $uiRoot
            }

            Invoke-Step -Name 'UI dependency audit' -Action {
                Invoke-Program -Command $npm.Source `
                    -Arguments @('audit', '--audit-level=high') `
                    -WorkingDirectory $uiRoot
            } | Out-Null
        }
        else {
            Add-ComponentSkip -Name 'UI tests' `
                -Reason 'dependency installation failed'
            Add-ComponentSkip -Name 'UI production build' `
                -Reason 'dependency installation failed'
            Add-ComponentSkip -Name 'UI Tauri frontend build' `
                -Reason 'dependency installation failed'
            Add-ComponentSkip -Name 'UI dependency audit' `
                -Reason 'dependency installation failed'
        }
    }
    else {
        Add-ToolchainSkip -Name 'UI tests/build/audit' `
            -Toolchain 'Node.js/npm'
    }
}
else {
    Add-ComponentSkip -Name 'UI tests/build/audit' `
        -Reason 'ui/package.json is not present'
}

$tauriManifest = Join-Path $repoRoot 'ui/src-tauri/Cargo.toml'
if (-not (Test-Path $tauriManifest)) {
    Add-ComponentSkip -Name 'Tauri backend tests' `
        -Reason 'standalone Tauri manifest is not present'
}
elseif (-not $cargo) {
    Add-ToolchainSkip -Name 'Tauri backend tests' `
        -Toolchain 'Rust/Cargo'
}
elseif (-not $tauriFrontendReady) {
    Add-ComponentSkip -Name 'Tauri backend tests' `
        -Reason 'Tauri frontend build did not complete'
}
else {
    Invoke-Step -Name 'Tauri backend tests' -Action {
        Invoke-Program -Command $cargo.Source `
            -Arguments @(
                'test',
                '--manifest-path',
                $tauriManifest,
                '--all-targets'
            ) `
            -WorkingDirectory (Split-Path -Parent $tauriManifest)
    } | Out-Null
}

$driverRoot = Join-Path $repoRoot 'driver'
$driverRunner = Join-Path $driverRoot 'tests/run-tests.ps1'
$driverStructuralTest = Join-Path $driverRoot 'tests/Test-SafetyInvariants.ps1'
if (Test-Path $driverRoot) {
    $cmake = Get-Tool -Names @('cmake.exe', 'cmake')
    $compilerAvailable = Test-CCompilerAvailable

    if ((Test-Path $driverRunner) -and $cmake -and $compilerAvailable) {
        Invoke-Step -Name 'Driver structural/native tests' -Action {
            Invoke-PowerShellScript -Path $driverRunner
        } | Out-Null
    }
    else {
        if (Test-Path $driverRunner) {
            Invoke-Step -Name 'Driver structural tests' -Action {
                Invoke-PowerShellScript -Path $driverRunner `
                    -ScriptArguments @('-SkipNative')
            } | Out-Null
        }
        elseif (Test-Path $driverStructuralTest) {
            Invoke-Step -Name 'Driver structural tests' -Action {
                Invoke-PowerShellScript -Path $driverStructuralTest
            } | Out-Null
        }
        else {
            Add-ComponentSkip -Name 'Driver structural tests' `
                -Reason 'no structural test script is present'
        }

        if (-not (Test-Path $driverRunner)) {
            Add-ComponentSkip -Name 'Driver native tests' `
                -Reason 'driver native test runner is not present'
        }
        elseif (-not $cmake) {
            Add-ToolchainSkip -Name 'Driver native tests' `
                -Toolchain 'CMake 3.20+'
        }
        elseif (-not $compilerAvailable) {
            Add-ToolchainSkip -Name 'Driver native tests' `
                -Toolchain 'C11 compiler (MSVC, Clang, or GCC)'
        }
    }
}
else {
    Add-ComponentSkip -Name 'Driver structural/native tests' `
        -Reason 'driver directory is not present'
}

$installerRoot = Join-Path $repoRoot 'installer'
if (Test-Path $installerRoot) {
    $installerStaticTests = @()
    $installerRunner = Join-Path $installerRoot 'tests/run-tests.ps1'
    $installerTestsRoot = Join-Path $installerRoot 'tests'
    if (Test-Path $installerRunner) {
        $installerStaticTests += (Resolve-Path $installerRunner).Path
    }
    else {
        $knownInstallerTests = @(
            (Join-Path $installerRoot 'tests/Test-Static.ps1'),
            (Join-Path $installerRoot 'tests/run-static-tests.ps1'),
            (Join-Path $installerRoot 'Test-Static.ps1')
        )

        foreach ($candidate in $knownInstallerTests) {
            if (Test-Path $candidate) {
                $installerStaticTests += (Resolve-Path $candidate).Path
            }
        }

        if (Test-Path $installerTestsRoot) {
            $discoveredStaticTests = Get-ChildItem -Path $installerTestsRoot `
                -Filter '*static*.ps1' -File -Recurse `
                -ErrorAction SilentlyContinue
            foreach ($test in $discoveredStaticTests) {
                $installerStaticTests += $test.FullName
            }
        }
    }

    $installerStaticTests = @($installerStaticTests | Sort-Object -Unique)
    if ($installerStaticTests.Count -eq 0) {
        Add-ComponentSkip -Name 'Installer static tests' `
            -Reason 'no static test script is present'
    }
    else {
        foreach ($test in $installerStaticTests) {
            Invoke-Step -Name "Installer static test: $(Split-Path -Leaf $test)" `
                -Action {
                    Invoke-PowerShellScript -Path $test
                } | Out-Null
        }
    }
}
else {
    Add-ComponentSkip -Name 'Installer static tests' `
        -Reason 'installer directory is not present'
}

Write-Host "`nTest summary"
foreach ($result in $results) {
    $detail = if ($result.Detail) { " - $($result.Detail)" } else { '' }
    Write-Host ("  {0,-7} {1}{2}" -f $result.Status, $result.Name, $detail)
}

$failures = @($results | Where-Object { $_.Status -eq 'FAILED' })
if ($failures.Count -gt 0) {
    Write-Host "$($failures.Count) test step(s) failed." -ForegroundColor Red
    exit 1
}

if ($Strict -and $missingRequiredToolchains.Count -gt 0) {
    $missing = @($missingRequiredToolchains.Keys | Sort-Object) -join ', '
    Write-Host "Strict mode requires unavailable toolchains: $missing" `
        -ForegroundColor Red
    exit 2
}

if ($missingRequiredToolchains.Count -gt 0) {
    Write-Host 'Required toolchains were skipped; rerun with -Strict to make skips fail.' `
        -ForegroundColor Yellow
}

exit 0
