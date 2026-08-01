# Rollback and recovery notes

This is a recovery design, not evidence that lifecycle testing has passed.
Validate every path in disposable Windows 11 x64 VMs and on each physically
qualified device before considering a release.

## Normal repair and uninstall

- Repair from the original signed MSI/bundle. Windows Installer restores
  package-owned files, then the custom-action DLL revalidates all hashes,
  signatures, versions, CAT membership, and INF IDs before idempotent driver
  staging. Existing user/machine mapping configuration is retained.
- Uninstall stops and removes the user-mode service. The custom-action DLL
  detaches/removes only the recorded product driver package. It must accept and
  surface a reboot-at-end result when Windows has the package in use.
- Configuration is retained by default. Deleting it must be a separate,
  explicit action after the service and driver are removed.

Keep an independent non-target keyboard and an administrative/recovery console
available while testing any driver lifecycle operation.

## Failed install or upgrade

The MSI queues rollback custom actions before driver staging/removal. A failure
after staging should detach the newly staged filter and restore the recorded
prior package. A major upgrade leaves the old package staged until the new
validated package has taken ownership, reducing the chance of a driverless or
mixed-ABI state.

If automatic rollback cannot restore a coherent package:

1. Stop the `MagicKeyboardService` service from an elevated recovery console.
2. Disconnect the affected target keyboard; keep an unrelated keyboard
   available.
3. Use Windows Settings/Apps or `msiexec /x {product-code}` with the original
   signed MSI. Honor any reboot request.
4. In Device Manager, inspect only the affected device instance and use the
   standard driver rollback/uninstall flow. Do not delete keyboard-class
   filters or change global class registry values.
5. If Windows cannot start normally, use Windows Recovery Environment/System
   Restore or restore the disposable-VM snapshot. Offline removal of a
   reviewed product driver package is a last-resort, release-specific procedure
   that must name the exact published INF recorded by the installer.

Never disable Secure Boot, integrity checks, signature enforcement, or enable
test signing as a recovery step.

## Required release drills

Before release, capture evidence for:

- clean install, repair, uninstall, same-version repair, supported upgrade, and
  downgrade refusal;
- interruption before/after file copy, service registration, driver staging,
  device bind, and reboot scheduling;
- invalid manifest/hash/signature/CAT/member/version/ABI/hardware-ID cases;
- rollback with target hardware present and absent, plus unrelated keyboards;
- locked files, service start failure, Driver Store failure, disk full, and
  pending reboot;
- WinRE recovery and post-uninstall keyboard operation.

Any missing drill, absent signed payload, or unapproved hardware binding is a
release blocker.
