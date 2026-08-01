# Native MSI custom-action contract

`InstallerActions.dll` is a required, signed release payload. It is not present
in this repository and must not be replaced by a script, `pnputil` command
line, empty DLL, or mock in an installable package.

The DLL must be a native x64 MSI custom-action DLL exporting these entry
points:

- `ValidatePayload`
- `StageDriverPackage`
- `RollbackDriverPackage`
- `CommitDriverPackage`
- `RemoveDriverPackage`
- `RollbackDriverRemoval`

Each entry point receives only MSI `CustomActionData`; deferred actions must
not read arbitrary MSI properties. Parse the data as a bounded, duplicate-free
key/value format with strict key allowlists and canonical absolute paths.
Reject malformed quoting, embedded NULs, reparse points, user-writable payload
locations, missing values, and paths outside `INSTALLFOLDER`/the protected
installer-state directory.

## Validation

`ValidatePayload` must independently implement or invoke equivalent native
checks for the installed, MSI-cabinet-restored files:

1. Verify the Authenticode-signed release manifest against release signer
   certificate SHA-256 pins compiled into the DLL and cross-check the packaged
   trust policy against those pins.
2. Verify every exact SHA-256, timestamped Authenticode signature, component
   version, protocol/ABI range, and driver CAT signature/membership.
3. Parse the INF and require the exact signed-manifest hardware-ID set:
   `HID\VID_05AC&PID_xxxx&MI_xx&COLxx`, with no wildcards, placeholder IDs,
   keyboard class registration, class GUID, or `UpperFilters` edits.
4. Require Windows x64 build 22621 or newer, elevation, normal code-integrity
   policy, and an approved device-binding evidence hash. Never weaken machine
   security to satisfy a failed check.
5. Return `ERROR_INSTALL_FAILURE` before service registration or driver
   servicing on any mismatch.

Source-controlled PowerShell is a release preflight and test oracle, not a
runtime trust anchor.

## Driver staging

`StageDriverPackage` must use documented SetupAPI/NewDev/Configuration Manager
driver-package APIs. It must:

- stage the exact validated INF/CAT/SYS into the Driver Store and bind only
  matching device instances already allowed by the INF;
- never create class-wide filters or registry `UpperFilters` values;
- record the published OEM INF name, package identity, prior package identity,
  devices changed, and reboot state in an ACL-protected state file;
- write state atomically (new file, flush, replace) and reject symlinks/reparse
  points;
- be idempotent for repair and reject conflicting/newer or ABI-incompatible
  state;
- report reboot-at-end through supported MSI APIs, never claim a reboot is
  unnecessary;
- leave unknown/unmatched devices untouched.

The MSI schedules `RollbackDriverPackage` immediately before staging.
Rollback must use the recorded identity, detach only devices changed by this
transaction, restore the previously recorded package where Windows servicing
permits, and request reboot when needed. If coherent restoration is impossible,
it must leave the device on the standard pass-through path and emit an
actionable MSI error/log entry.

`CommitDriverPackage` runs only after MSI commit. It may then remove a
superseded package recorded by staging, but only after revalidating both
identities and confirming no unrelated device uses it. A failure to clean an
old Driver Store package must be reported for maintenance; it must not undo the
successfully committed current package or force-delete an in-use package.

## Uninstall and rollback

`RemoveDriverPackage` must identify the package from protected state and
revalidate it against the product/INF identity. It must detach the filter from
only the recorded matching device instances before attempting Driver Store
deletion. A package in use may require reboot; do not force-delete files or
unrelated packages.

`RollbackDriverRemoval` is scheduled before removal and must restage/rebind the
same validated package if a later MSI action rolls back. Major-upgrade removal
sets `UPGRADINGPRODUCTCODE`; the MSI intentionally leaves the old driver staged
so the incoming package can replace it transactionally.

All functions must use bounded logs without raw HID reports, typed input,
secrets, or certificate material. Signing keys and test certificates must
never be embedded.
