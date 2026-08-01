# Transient Tauri settings shell

This crate is an on-demand Tauri v2 host for the existing settings frontend. It
is intentionally **not** the Windows keyboard service: it has no keyboard
input path, tray, autostart, action execution, or persistent configuration.
Its state is an in-memory mock and disappears when the window closes.

From `ui/`, build the browser UI with `npm run build` or the native executable
with:

```powershell
npm run build:tauri
```

The Tauri build uses a separate frontend entry that installs the public Tauri
`invoke` bridge before loading the application. The normal Vite build continues
to use the TypeScript mock adapter.

## Security and lifetime

- The webview loads only bundled `frontendDist` assets. A navigation guard
  accepts only Tauri's local application origins.
- The CSP blocks remote scripts, frames, objects, forms, and network
  connections. Its only non-`self` connection sources are Tauri's local IPC
  transports.
- `capabilities/settings-window.json` grants the `main` window only the six
  commands in `ui/README.md`. No shell, process, filesystem, or broad core
  permission is granted.
- The native close event asks Tauri's application handle to exit. There is no
  hidden window or resident tray process.
- `launch` mappings are only syntax-validated and stored in memory. This shell
  never resolves or executes their targets.

## Replacing mock state with service IPC

Keep the six Tauri command signatures as the webview boundary, but replace
`SettingsState` access inside the command handlers with a small service client.
The Windows service remains authoritative for discovery, active policy state,
validation, persistence, and action authorization; the device-bound driver is
the only intended input-filter path.

A conservative named-pipe transport can use this shape:

1. Let the service create a versioned local pipe such as
   `\\.\pipe\MagicKeyboard.Service.v1`. Apply a DACL limited to the owning
   interactive user and `SYSTEM`, reject remote clients, and verify the client
   SID after connection.
2. Use bounded, length-prefixed messages rather than newline framing. Include
   `protocolVersion`, a random `requestId`, one of the six method names, and its
   typed payload. Set a small maximum frame size (for example 1 MiB), read/write
   deadlines, and a maximum number of in-flight requests.
3. Have the service return either the existing camel-cased response type or a
   stable, user-safe error code/message. Do not forward OS errors, pipe paths,
   executable paths, or internal diagnostics to the webview.
4. Serialize calls that mutate configuration, and make the service run the same
   profile validation before applying or persisting a change. In particular,
   treat `launch` targets as untrusted input and apply service-side allowlisting
   or native confirmation before execution.
5. On a missing or disconnected service, fail the command promptly with a safe
   “service unavailable” error. Reconnect on the next user action; do not keep
   this UI host alive or add a background polling loop.
6. Add contract tests with a fake pipe server, then integration tests proving
   that malformed frames, oversized payloads, unknown methods, wrong-user
   clients, and timeouts fail closed.

The shell must never send a service shutdown request when its window closes.
Its process exit only tears down the transient settings webview.
