# MagicKeyboard A1644 HID lower filter

This directory contains a **WDM HIDCLASS lower filter** for Apple Magic Keyboard
**A1644** (`USB\VID_05AC&PID_0267&MI_01` and Bluetooth
`BTHENUM\{00001124-...}_VID&0001004c_PID&0267`).

Architecture and A1644 rewrite logic are adapted from
[WinAppleKey](https://github.com/samartzidis/WinAppleKey) (MIT License,
Copyright (c) 2021 George Samartzidis). See `../third_party/NOTICE.md`.

## Why a lower filter?

On A1644, Eject is bit 0 of the last byte of the keyboard HID payload. Windows
opens that collection exclusively and strips the bit before user-mode (Raw
Input / hooks) can see it. WinAppleKey’s approach — and ours — rewrites the
USB/Bluetooth transfer buffer in a completion routine **before** HIDCLASS
parses the report, turning Eject into a normal Delete usage (`0x4C`).

Default registry settings (service key):

| Value | Default | Meaning |
| --- | --- | --- |
| `SwapFnCtrl` | `1` | Physical Ctrl acts as Fn; Fn acts as Left Ctrl |
| `SwapAltCmd` | `0` | Leave Alt/Cmd unswapped |

## Host tests (no WDK)

```powershell
cd driver
.\tests\run-tests.ps1
```

## WDK build

1. Install Visual Studio + Windows Driver Kit.
2. Open `MagicKeyboardFilter.vcxproj` and build **x64 Release**.
3. Test-sign the `.sys` / package (or enable test signing):

```text
bcdedit /set testsigning on
```

(Requires Secure Boot off on many machines; reboot after changing.)

4. Install the INF against the A1644 USB or Bluetooth HID device (Device Manager
   → Update driver → Have disk), or use `pnputil /add-driver MagicKeyboardFilter.inf /install`.
5. Disconnect/reconnect the keyboard (or reboot).

Keep a second keyboard available. This filter sits under the inbox HID stack; a
bad build can break this keyboard until the driver is removed.

## Deliberate limitations

- Validated path is **A1644 only** (same scope as WinAppleKey).
- No production attestation signature is included.
- Older KMDF keyboard upper-filter scaffold files (`control.c`, `device.c`,
  `input.c`, mapping IOCTL headers) remain for host mapping experiments but are
  **not** linked into the WDM `.sys`.
