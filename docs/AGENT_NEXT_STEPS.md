# Agent next steps — MagicKeyboard

> Ingest this before continuing work. Last updated: 2026-07-31.
> Repo: `C:\Users\cool_\Documents\MagicKeyboard` → `https://github.com/Ibrahim-Al-Omran/MagicKeyboard`
> HEAD (local): `26a8ac1` — *Initial Magic Keyboard for Windows prototype.*
> **Push status:** commit exists locally; `origin/main` was `[gone]` — **push still required** (`git push -u origin main`).

---

## Goal

Ship Magic Utilities–like remapping for Apple Magic Keyboard on Windows.
**A1644 Eject→Delete** requires a kernel HID lower filter; user-mode alone cannot see Eject.

---

## Current state (what already works)

| Area | Status |
| --- | --- |
| Tauri settings UI + F-row remaps via `WH_KEYBOARD_LL` | Working (dedicated message-pump thread) |
| Auto-detect A1644 BT `004C:0267` as compact (not Touch ID) | Working |
| Right Ctrl = software Fn invert; Caps Lock→Delete interim | Working |
| HID probe / Raw Input diagnostics | Working; proves Eject invisible in user-mode |
| A1644 WDM HIDCLASS lower filter (WinAppleKey-inspired) | **Built** `driver\x64\Release\MagicKeyboardFilter.sys` |
| Host rewrite/mapping tests | Passing (`driver\tests\run-tests.ps1`) |
| WDK kits + VS `WindowsKernelModeDriver10.0` toolset | Installed / fixed on this machine |

### Why Eject failed in the app

A1644 nests **Eject (consumer `0xB8`)** in the boot keyboard HID report. Windows exclusive-opens that collection → Raw Input only gets `RIM_TYPEKEYBOARD` (bit stripped). Descriptor tools “see” Eject; AHK scripts that work on older AWK use a **separate** `RIM_TYPEHID` collection this BT keyboard does not expose for Eject.

WinAppleKey (and our filter) intercept **Bluetooth ACL / USB URB** completion and rewrite the 9-byte payload before HIDCLASS parses it.

---

## Immediate next steps (priority order)

### 0. Push git (if not done)

```powershell
cd C:\Users\cool_\Documents\MagicKeyboard
git push -u origin main
```

If remote rejects or repo is empty/protected, confirm GitHub auth (`gh auth status`) and that `origin` points at the intended repo.

### 1. Enable test signing + install the filter

**Risk:** Secure Boot may block `bcdedit testsigning`. Keep a second keyboard. Snapshot/VM preferred.

```powershell
# Admin
bcdedit /set testsigning on
# reboot

# After reboot — install INF (adjust path if package layout differs)
pnputil /add-driver C:\Users\cool_\Documents\MagicKeyboard\driver\x64\Release\MagicKeyboardFilter.inf /install
```

Hardware IDs in INF (must match Device Manager):

- USB: `USB\VID_05AC&PID_0267&MI_01`
- BT: `BTHENUM\{00001124-0000-1000-8000-00805f9b34fb}_VID&0001004c_PID&0267`

User’s live BT HID instance matches the BTHENUM ID pattern (possibly two paired Magic Keyboards — bind carefully).

After install: disconnect/reconnect keyboard (or reboot). Confirm `MagicKeyboardFilter` in device stack (lower filter on Bluetooth HID Device).

### 2. Verify Eject → Delete

1. Open Notepad; press **Eject** — expect Delete behavior.
2. Optional: Event Viewer / DebugView if rewrite seems inactive.
3. Registry service values (defaults match WinAppleKey):
   - `HKLM\SYSTEM\CurrentControlSet\Services\MagicKeyboardFilter\SwapFnCtrl` = `1`
   - `...\SwapAltCmd` = `0`
4. Once kernel path works: re-enable Eject in UI (`ui/src/keyboard-models.ts` — currently `unavailable: true`); remove/soften Caps Lock→Delete as sole workaround; update A1644 banner in `ui/src/app.ts`.

### 3. Rebuild hygiene (if `.sys` missing)

```powershell
# Requires VS 2026 + WDK + Component.Microsoft.Windows.DriverKit toolset
& "C:\Program Files\Microsoft Visual Studio\18\Community\MSBuild\Current\Bin\MSBuild.exe" `
  C:\Users\cool_\Documents\MagicKeyboard\driver\MagicKeyboardFilter.vcxproj `
  /p:Configuration=Release /p:Platform=x64 /p:SkipPackageVerification=true
```

Notes from last successful build:

- `SkipPackageVerification=true` — InfVerif x86 DLL path broken on this kit install.
- Do **not** `#define _KERNEL_MODE` in the vcxproj (reserved; WDK defines it).
- Shared rewrite: `driver/src/a1644.c` + `driver/include/mkfilter_a1644.h` (host + kernel).

If toolset missing again: install VS Individual Component **Windows Driver Kit** (`Component.Microsoft.Windows.DriverKit`), or re-run `driver/out/install-wdk-vs-integration.ps1` elevated (may exist under `driver/out/` — that dir is gitignored).

---

## Architecture map (for agents)

```
User-mode (ui/src-tauri)
  remap.rs          LL hook + SendInput actions
  raw_input.rs      Raw Input + Eject probe (documents Access Denied)
  fn_watch.rs       Vendor HID ff00:0014 (no Eject)
  detect.rs         Model inference (0267 = compact A1644)

Kernel (driver/)
  src/driver.c      WDM lower filter; BTH/USB completion rewrite
  src/a1644.c       ProcessA1644Payload (Eject→Del, Fn/Ctrl, Alt/Cmd)
  MagicKeyboardFilter.inf   HIDClass lower filter HWIDs

Reference
  third_party/WinAppleKey/  MIT source (attribution in NOTICE.md)
```

**Do not** reverse-engineer Magic Utilities binaries. WinAppleKey MIT reuse with attribution is OK.

---

## Follow-on product work (after Eject verified)

1. **Installer packaging** — ship test-signed `.sys`+INF via NSIS/WiX with clear Secure Boot / testsigning UX (`installer/`, `ui` package scripts).
2. **UI/driver policy sync** — expose SwapFnCtrl / SwapAltCmd in settings; optional disable of Caps Lock→Delete when filter present.
3. **Detection of filter presence** — settings banner: “filter installed” vs “user-mode fallback”.
4. **Production signing** — attestation / EV cert (blocked without company + cost); document clearly.
5. **Win11 validation** — WinAppleKey README claimed Win10-only; validate our port on this Win11 machine after install.
6. **Duplicate BT devices** — user had two `004C:0267` Magic Keyboard Bluetooth nodes; ensure filter attaches to the active one.
7. **Remove dead KMDF scaffold confusion** — `control.c` / `device.c` / `input.c` remain for mapping experiments but are **not** in the WDM `.sys`; either delete or quarantine under `driver/legacy/` to avoid future agents wiring the wrong stack.
8. **CI** — host tests only today; do not load drivers in CI. Keep InfVerif optional until kit path fixed.

---

## Explicit non-goals / traps

- Do not expect Raw Input / AHK / SharpKeys to see A1644 BT Eject.
- Do not attach as keyboard-class upper filter for Eject — wrong layer.
- Do not commit `**/target/`, `node_modules/`, `driver/out/`, `driver/x64/`, `MagicKeyboard.exe`, `releases/` (see root `.gitignore`).
- `hid_listen.rs` exists but is **not** started from `lib.rs` (keyboard open Access Denied loop).

---

## Quick verify commands

```powershell
# Apple / Magic PnP
Get-PnpDevice | Where-Object { $_.InstanceId -match '004C.*0267|05AC.*0267' }

# Raw Input device list helper
cd tools\raw-list; cargo run --release

# Host driver tests (no WDK)
cd driver; .\tests\run-tests.ps1

# Describe A1644 HID collections
cd tools\hid-probe; cargo run --release -- describe --vid 0x004c --pid 0x0267 --index 0
```

---

## Acceptance criteria for “Eject done”

- [ ] Filter service loaded on A1644 BT (and/or USB) stack
- [ ] Physical Eject produces Delete in a text editor without Caps Lock remap
- [ ] Settings UI marks Eject available / maps Eject→Delete as first-class binding
- [ ] README documents testsigning + uninstall recovery
- [ ] `main` pushed to GitHub
