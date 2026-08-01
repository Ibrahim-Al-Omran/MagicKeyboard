# MagicKeyboard

> **Warning: this repository is an early prototype and is not production
> software. Do not install the filter driver on a physical system or rely on
> it for keyboard input. No keyboard model, transport, installer, or release
> artifact is currently supported.**

MagicKeyboard is a Windows prototype exploring narrowly device-bound remapping
for selected Apple Magic Keyboard models. The repository currently contains
platform-neutral mapping/service code, an A1644 HIDCLASS lower filter adapted
from WinAppleKey (see `third_party/NOTICE.md`), a developer HID probe, and a
settings UI. The production architecture and acceptance criteria are described in
[the component specification](docs/COMPONENTS.md).

## Quick test

From a Windows PowerShell prompt:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\test.ps1
```

The orchestrator detects installed toolchains and reports unavailable
production toolchains as `SKIPPED`. Use `-Strict` to make those missing
toolchains produce a nonzero exit code, as CI does. Ordinary test, build, or
audit failures always produce a nonzero exit code.

## Repository layout

- `crates/` — Rust workspace crates for core mapping, adapter policy, and the
  service shell.
- `tools/hid-probe/` — standalone Rust HID diagnostics and offline trace tool.
- `ui/` — TypeScript/Vite settings prototype plus a standalone Tauri host.
- `driver/` — A1644 HIDCLASS lower filter (WinAppleKey-inspired) plus host-only
  C mapping / rewrite tests. See `driver/README.md` and `third_party/NOTICE.md`.
- `installer/` — fail-closed Windows installer/updater scaffold and static
  fixture tests.
- `docs/` — architecture, safety constraints, and component requirements.
- `scripts/` — repository-level developer and CI orchestration.
- `.github/workflows/` — Windows CI configuration.

There is no production installer, signed driver package, validated hardware
allowlist, or completed remapping path. CI runs only unprivileged host tests
and static checks; it does not install or load a test driver.
