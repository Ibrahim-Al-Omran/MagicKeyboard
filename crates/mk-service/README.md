# mk-service

`mk-service` wires a device source, the `mk-core` mapping engine, an action sink,
and driver lifecycle control through synchronous traits. This keeps native I/O
replaceable and makes orchestration testable without hardware.

Console mode is the currently functional adapter:

```text
mk-service --console --config magic-keyboard.json
```

It reads one `SourceEvent` JSON object per stdin line and writes one
`ActionDispatch` JSON object per action to stdout. Blank input lines are ignored
and EOF ends the run. For example:

```json
{"event":{"device_id":"keyboard","sequence":1,"timestamp_micros":0,"kind":"button","control":{"usage_page":7,"usage":4},"state":"pressed","repeat":false}}
```

The `cfg(windows)` module only defines explicit integration seams. Native
Windows service registration, HID report decoding, action execution, and
driver control remain unimplemented.
