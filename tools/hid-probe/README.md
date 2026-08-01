# hid-probe

`hid-probe` is a Windows-focused, read-only HID recorder for clean-room
interoperability research on keyboards you own or are authorized to inspect. It
enumerates HID interfaces, retrieves metadata and report descriptors through
`hidapi`, records unsolicited input reports, and analyzes traces without
hardware.

The implementation has no command that sends HID output or feature reports.
Capture only calls the HID input-read API. A future device-write feature must be
a separately named command with an explicit per-invocation danger opt-in; it
must never be enabled by a default, a config file, or device detection.

## Build and test

Install the stable Rust MSVC toolchain and the Visual Studio C++ build tools,
then run from this directory:

```powershell
cargo build --release
cargo test
```

This directory has its own `Cargo.toml` and is not part of a root workspace.
Offline decode is portable; live `list`, `describe`, and `capture` commands
return an explanatory error on non-Windows systems.

## USB capture workflow

1. Plug the keyboard directly into the Windows machine with a data-capable
   cable. If both Bluetooth and USB entries remain visible, use the `bus` and
   filtered index shown by `list`.
2. Enumerate Apple's interfaces (Apple's VID is `05ac`):

   ```powershell
   .\target\release\hid-probe.exe list --vid 05ac
   ```

3. Record the relevant PID and zero-based filtered index. Magic Keyboard models
   are composite devices, so several interfaces can share a VID/PID.
4. Inspect one interface without sending a report:

   ```powershell
   .\target\release\hid-probe.exe describe --vid 05ac --pid 0267 --index 0
   ```

5. Capture while performing a short, predetermined key sequence:

   ```powershell
   .\target\release\hid-probe.exe capture --vid 05ac --pid 0267 --index 0 `
     --duration-seconds 15 --output .\usb-sample.jsonl
   ```

Replace `0267` with the PID reported for your keyboard. The destination must
not already exist. Use `--max-reports` for a report-count limit or Ctrl-C for a
graceful stop.

## Bluetooth capture workflow

1. Pair the owned keyboard in Windows Settings, connect it, and press a harmless
   key to ensure it is awake.
2. Run `list --vid 05ac` and select an entry whose `bus` is `bluetooth`. The
   Windows Bluetooth HID stack may expose multiple top-level collections.
3. Run `describe` and `capture` exactly as above, using that entry's PID and
   filtered index. Avoid simultaneously connecting the cable if it makes the
   chosen index ambiguous.
4. Keep the keyboard awake during capture. A disconnect, sleep transition, or
   exclusive handle held by another program can end the read with an error.

No administrator privileges are normally required. Windows policy or another
driver can still deny a handle.

## Offline decode and trace format

Analyze a checked-in fixture or a captured file without any HID device:

```powershell
cargo run -- decode tests\fixtures\sample-v1.jsonl
cargo run -- decode tests\fixtures\sample-v1.jsonl --reports
cargo run -- summarize tests\fixtures\sample-v1.jsonl --json
```

Decode validates the schema, record ordering, sequence numbers, hex lengths,
and footer count. It summarizes report lengths, report IDs, timing, and changed
byte positions. `--reports` separates a descriptor-declared report ID from each
payload. It intentionally does not guess Apple-specific key meanings.

Each JSONL line carries:

- `schema: "hid-probe.trace"` and `schema_version: 1`;
- one `header`, `input_report`, or `footer` record;
- wall-clock microseconds plus monotonic elapsed microseconds for reports;
- lowercase hexadecimal input bytes;
- redacted device metadata and the safely retrieved report descriptor summary.

The writer flushes each line so an interrupted file generally remains useful.
A missing footer is reported as an incomplete capture.

## Identifier and keystroke privacy

Serial numbers and Windows device paths are omitted by default from list,
describe, and trace output. `device_ref` is only a filtered-list index, not a
stable hash. `--include-identifiers` is an explicit disclosure option and emits
a warning.

Input traces can reveal everything typed, including passwords. Use an empty test
document and a fixed non-secret sequence, stop capture before normal typing,
restrict access to trace files, and remove traces when they are no longer
needed.

## Legal and safety constraints

- Use only hardware you own or have explicit permission to inspect.
- Limit the work to lawful interoperability, compatibility, testing, or
  research in your jurisdiction. This tool is not legal advice.
- Do not use it to bypass access controls, authentication, encryption, device
  management, or platform security.
- Do not collect another person's keystrokes or identifiers without informed
  consent.
- Keep clean-room notes: separate observed facts and traces from any independent
  implementation, and do not copy proprietary firmware or leaked
  documentation.
- The tool is intentionally passive at the HID report layer. It cannot change
  LEDs, modes, pairing state, firmware, or device configuration.

## Hardware and API limitations

- Windows HID APIs expose logical reports, not raw USB transactions, Bluetooth
  radio packets, transport encryption, or exact bus timestamps.
- Report descriptors can be unavailable, denied, or synthesized by the Windows
  stack, especially for Bluetooth and composite devices.
- Timestamps are taken in user space after `read_timeout` returns. Driver
  buffering and scheduler latency remain.
- The OS keyboard driver may also consume the same input, and some interfaces
  cannot be opened concurrently.
- Generic decode reports bytes, IDs, frequencies, and changes; it does not yet
  evaluate HID usages or Apple-specific semantics.
- Because no feature/output reports are requested or sent, protocols that
  require an active mode switch are deliberately out of scope.
