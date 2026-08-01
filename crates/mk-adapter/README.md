# mk-adapter

`mk-adapter` is a fail-closed compatibility boundary between discovered Apple
HID devices and bounded driver policy. It does not open devices or install a
driver.

## Current support status

The built-in support registry is **empty**. This crate contains no claimed
Apple product IDs, real descriptor fingerprints, or production adapters. The
only registry entries are synthetic, sanitized fixtures compiled into unit
tests. `built_in_registry()`, `SupportRegistry::default()`, and
`SupportRegistry::empty()` therefore reject every device.

A device can be selected only when all of these captured values match one
qualified definition:

- Apple vendor ID;
- exact product ID;
- exact transport (USB, classic Bluetooth, or Bluetooth Low Energy); and
- SHA-256 of the complete raw HID report descriptor.

Names, serial strings, neighboring product IDs, and similar descriptors are
not fallback signals. Unknown fingerprints and duplicate matches are
unsupported.

## From `hid-probe` fixture to adapter

Probe output is evidence, not support. A fixture graduates only through this
process:

1. Capture with `hid-probe` from identified physical hardware. Preserve the
   complete raw report descriptor, product ID, transport, interface metadata,
   probe version, and hardware/firmware revision. Remove serial numbers,
   Bluetooth addresses, host paths, and other owner-specific data.
2. Repeat captures across reconnects and every transport that will be
   supported. Transport variants receive separate definitions even when their
   descriptors happen to be equal.
3. Review descriptor parsing and manually declare the minimal normalized input
   and output controls plus report-length bounds. Never infer capabilities from
   the model name or product-ID family.
4. Add sanitized fixture tests with a checked-in golden fingerprint, exact
   selection, one-byte descriptor-change rejection, and capability-negative
   cases.
5. Qualify on hardware: verify every declared input, report size and state
   transition; run suspend/resume, reconnect, pairing, rollover, malformed
   report, and transport-specific tests; then exercise the compiled policy
   against the intended driver version.
6. Record review and hardware qualification before adding a production
   `AdapterDefinition`. A fixture or passing unit test alone must never be
   promoted as hardware support.

If captures disagree, remain fixtures and investigate. Do not add multiple
equivalent definitions to make selection succeed: an ambiguous exact match is
rejected by design.

## Policy compilation

`compile_policy` accepts only one-to-one mappings whose source and destination
controls are declared by normalized capabilities. It rejects duplicate inputs,
duplicate outputs, unsupported controls, and operation counts above both the
caller-selected bound and `MAX_DRIVER_POLICY_OPERATIONS`. `DriverPolicy` fields
are private, so unvalidated operation vectors cannot be constructed.

## Local verification

This crate is a member of the root Rust workspace. From the repository root:

```text
cargo fmt --all -- --check
cargo test -p mk-adapter
```
