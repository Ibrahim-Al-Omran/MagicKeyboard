# Windows installer/updater scaffold

This directory is a WiX v4-oriented, fail-closed scaffold. It is **not a
production-ready installer** and cannot build from the repository's current
artifacts. In particular, the checked-in driver INF contains placeholder
hardware IDs, no signed driver package exists, the service is not a native
Windows service yet, the Tauri UI host is only an in-memory prototype, and no
release UI executable or MSI custom-action DLL exists.

The supported release build path refuses to package placeholders. It requires:

- an Authenticode-signed release manifest and exact SHA-256 for every payload;
- an externally configured trust policy with pinned signer-certificate
  SHA-256 values (not signer names supplied by the manifest);
- valid, timestamped Authenticode signatures on the service, UI host, driver
  SYS, and installer custom-action DLL;
- a valid, timestamped driver catalog signature plus `signtool /kp /c`
  membership checks for the exact INF and SYS;
- compatible UI protocol and driver ABI versions;
- an approved, hash-bound device-binding review and exact, non-placeholder
  `HID\VID_05AC...` INF IDs;
- Windows 11 x64 build 22621 or newer and an elevated per-machine install.

No script here disables Secure Boot, code-integrity policy, driver-signing
enforcement, or test-signing requirements. The MSI uses a device package
helper contract; it never writes keyboard-class `UpperFilters`.

## Layout

- `manifests/`: deliberately invalid examples for release and offline update
  metadata.
- `policy/`: deliberately unconfigured signer trust policy.
- `scripts/`: manifest, signature, hash, version, INF, and update validation.
- `tests/`: pure PowerShell fixture tests; WiX and signing tools are not
  required.
- `wix/`: WiX v4 MSI and offline Burn bundle authoring.
- `RECOVERY.md`: rollback, uninstall, repair, and recovery constraints.

## Expected payload

The payload root must contain only the files declared in the signed release
manifest:

```text
Service/MagicKeyboard.Service.exe
UI/MagicKeyboard.Settings.exe
Driver/MagicKeyboardFilter.inf
Driver/MagicKeyboardFilter.cat
Driver/MagicKeyboardFilter.sys
Installer/InstallerActions.dll
Evidence/device-binding-review.json
```

Do not substitute scripts, empty files, renamed development builds, or other
placeholders. `InstallerActions.dll` is intentionally absent; its required
transaction and driver-servicing behavior is specified in
`wix/InstallerActions-CONTRACT.md`.

## Validate and build

Run fixture tests:

```powershell
.\installer\tests\run-tests.ps1
```

For a real release candidate, first create signed manifests and a configured
trust policy outside the example files. `signtool.exe` must be on `PATH`:

```powershell
.\installer\scripts\Test-ReleasePayload.ps1 `
  -PayloadRoot C:\release\payload `
  -ManifestPath C:\release\release-manifest.psd1 `
  -TrustPolicyPath C:\release\trust-policy.psd1
```

The WiX projects run the same preflight before compilation:

```powershell
dotnet build .\installer\wix\Product.wixproj `
  -p:PayloadRoot=C:\release\payload `
  -p:ReleaseManifest=C:\release\release-manifest.psd1 `
  -p:TrustPolicy=C:\release\trust-policy.psd1
```

Build and sign the MSI before building/signing the bundle. Then create and
Authenticode-sign an update manifest containing the final signed bundle hash.
`Test-UpdatePackage.ps1` validates an offline bundle and rejects rollback
sequences before any launch. There is no network downloader or unattended
always-running updater in this scaffold.

Direct `wix build` invocations are unsupported: the project preflight creates a
generated include and a private staged tree only after all release checks pass.
The example manifests and policy are intentionally invalid and must never be
edited into apparently usable defaults.

## Service, repair, and uninstall behavior

The MSI installs the service as `LocalSystem`, automatic start, and configures
bounded recovery (restart twice, then no action). The settings UI is installed
as an on-demand shortcut and is not registered as a startup program.

Windows Installer repair restores package-owned files and registrations while
leaving user/machine configuration untouched. Uninstall stops/removes the
service and asks the custom-action DLL to detach/remove only the exact
driver-package identity recorded during staging. Configuration is retained;
deletion must be a separate, explicit user/admin action.

Successful fixture tests prove only the offline validation logic. They do not
qualify a driver, hardware model, service, UI host, MSI lifecycle, signing
pipeline, or recovery procedure.
