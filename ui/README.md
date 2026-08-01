# Magic Keyboard settings UI

A lightweight, on-demand settings frontend for a native keyboard service. It is
plain TypeScript and DOM APIs, with Vite for building and Vitest for tests. The
default browser build uses a stateful mock IPC adapter, so no Rust backend is
needed for development or tests. The Tauri host in `src-tauri/` also uses
in-memory fixture state; it is not connected to `mk-service`.

## Run it

```powershell
cd C:\Users\cool_\Documents\MagicKeyboard\ui
npm install
npm test
npm run build
npm run dev
```

`npm run dev` models two devices, battery and connection states, function lock,
profile selection, editable key mappings, and all supported action kinds. Mock
changes live only for the current page session.

## Process architecture

In the intended architecture, the inactive device-bound driver owns the input
filter path and the native service owns device policy, persisted configuration,
tray state, and background lifetime. No production keyboard-hook backend is
part of this repository.

The eventual service tray should launch this UI on demand. The current Tauri
host exits when its settings window closes. It does not start, stop, or
communicate with a service. The frontend has no background polling loop, hidden
window, tray implementation, or persistent state.

For strict process isolation, package the settings window as a small transient
Tauri executable launched by the service. A single resident Tauri process can
destroy its settings webview when closed, but the native process will remain
alive; that is acceptable only when that process is itself the native service,
not a resident web UI.

## IPC boundary

`src/ipc.ts` is the contract shared by the screen, mock, and native adapter.
`src-tauri/` registers these Tauri commands against in-memory fixture state:

- `get_snapshot({}) -> AppSnapshot`
- `refresh_devices({}) -> AppSnapshot`
- `select_device({ deviceId }) -> AppSnapshot`
- `set_fn_lock({ deviceId, enabled }) -> DeviceStatus`
- `set_active_profile({ deviceId, profileId }) -> DeviceStatus`
- `save_profile({ profile }) -> AppSnapshot`

Native response structs should use `#[serde(rename_all = "camelCase")]`. Match
Tauri command argument casing to the camel-cased payloads above. Native errors
should be safe user-facing strings or serializable errors; the adapter converts
them to `IpcError`.

Configuration is validated in `src/validation.ts` before the mock saves it. The
native service remains authoritative and must run equivalent validation before
persisting or applying any mapping.

## Tauri integration status

The Tauri v2 crate, restricted capability file, separate frontend entry, and
six command handlers are present. `npm run build:tauri:frontend` verifies the
bridge-specific bundle, while `npm run build:tauri` builds the transient host.

The command payloads are a UI prototype contract, not the `mk-core`
configuration schema. Before this host can be integrated, the service needs a
versioned local IPC implementation and an explicit conversion layer. The
in-memory Rust state must then be replaced by that client, and service-side
authorization must define which mappings are actually representable. Platform
icons, signing, installer payloads, and native end-to-end tests also remain.

## Current limitations

- The Rust commands and native window lifecycle use fixture state only. There
  is no service IPC, persistence, tray, release installer, or signed package.
- Device refresh is user initiated; native event subscriptions can be added if
  an open settings window needs live updates.
- Key names and application targets are syntactically validated. Platform key
  availability, executable resolution, permissions, and action execution must
  be validated by the service.
- The mock adapter is intentionally in-memory and does not emulate hardware
  timing or operating-system failures beyond basic disconnected-device errors.
