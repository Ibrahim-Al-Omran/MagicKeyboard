# Magic Keyboard Utility — Component Specification

## 1. Purpose and scope

This document specifies a lightweight Windows utility that remaps selected keys on explicitly supported Apple Magic Keyboard models. The production design is a narrowly scoped system utility, not a general keyboard automation platform.

The words **MUST**, **SHOULD**, and **MAY** are normative. A device, Windows release, architecture, model, or transport is supported only when it appears in the release support matrix and has passed the required hardware tests.

### Product-wide goals

- Remap supported keys before Windows applications receive the original key event, so a consumed source key does not leak through.
- Affect only an explicitly allowlisted Apple model, transport, HID collection, and descriptor fingerprint. Other keyboards MUST behave exactly as they did before installation.
- Keep kernel code minimal. Model interpretation, configuration, diagnostics, and UI logic belong in memory-safe user mode where possible.
- Continue normal keyboard input when the service, UI, configuration, or update path fails. Unsupported or invalid states MUST fail open to an unmodified keyboard path.
- Apply a saved mapping at boot and device reconnect without requiring the settings UI to run.
- Preserve low latency and normal rollover, repeat, modifier, lock-key, sleep/wake, reconnect, and multi-key behavior.
- Require no network connection for input handling or configuration.
- Make every supported model/transport combination reproducible from a documented HID descriptor/report fixture and a physical-device qualification result.
- Ship only signed, version-compatible artifacts through an auditable installer/update path.

### Required technical constraints

- Windows Raw Input can identify the source device for events delivered to an application, but it observes input after the normal input path and **cannot suppress** the original keystroke system-wide. It is suitable for discovery and diagnostics, not the production remapping backend.
- A `WH_KEYBOARD_LL` low-level keyboard hook does not reliably provide stable per-device identity. Hooks are also timing-sensitive, run in user-session context, and cannot provide the required boot, secure-desktop, or service reliability. They MUST NOT be the production remapping backend.
- Production suppression/remapping requires a signed KMDF keyboard/HID filter attached only to the supported device stack. A class-wide keyboard filter or an INF match broad enough to capture unrelated keyboards is prohibited.
- User-mode `SendInput` MUST NOT be used to compensate for an unsuppressed source key. Any later synthetic-input design requires an explicit threat model, loop prevention, and a separately reviewed architecture.

## 2. Architecture and data flow

```text
Supported Apple keyboard
    │ USB or Bluetooth HID reports
    ▼
Device-bound signed KMDF filter
    │ validates device/descriptor, suppresses or rewrites allowed usages
    ├──────────────────────────────────────────────► Windows HID/keyboard stack
    │                                                (applications receive result)
    │ device interface + versioned IOCTL ABI
    ▼
Rust Windows service (LocalSystem)
    │ selects adapter, compiles/validates policy, owns persisted configuration
    ├──────────────► Apple model + transport adapter layer
    ├──────────────► opt-in recorder/probe diagnostics
    │ secured, versioned named-pipe IPC
    ▼
Tauri settings/tray UI (interactive user)

Signed installer/updater installs and services all release artifacts.
Automated tests and a physical hardware matrix gate releases.
```

### Startup and configuration flow

1. The installer registers the driver package and the auto-start service. The UI is optional at boot.
2. On device arrival, the filter verifies that the device identity and HID descriptor match its allowlist. Until a valid policy is loaded, reports pass through unchanged.
3. The service enumerates filter device interfaces, reads non-secret device metadata and descriptor fingerprints, and selects an exact model/transport adapter.
4. The service loads and migrates persisted configuration, validates it against the contract, and asks the adapter to compile normalized mappings into a bounded driver policy.
5. The driver validates ABI version, target device instance, descriptor fingerprint, report bounds, operation count, and allowed transformation types before atomically replacing the active policy.
6. The UI reads and writes configuration only through the service. It never opens the driver directly and is not required for remapping.

### Input flow

1. A supported keyboard emits an input report.
2. The filter parses only the report fields authorized by its validated policy.
3. Unmapped usages pass through unchanged. Mapped usages are suppressed or rewritten without disturbing unrelated usages in the same report.
4. Reports that are malformed, unknown, out of bounds, or associated with a stale policy pass through unchanged and increment a rate-limited diagnostic counter.
5. Windows receives one coherent result; the original source usage is not also delivered.

### Trust boundaries

- HID report bytes, descriptors, UI requests, persisted configuration, update metadata, and driver IOCTL payloads are untrusted inputs.
- The driver trusts no variable-length offset, report length, operation count, or device identity supplied by user mode without independent validation.
- The service is the only production caller authorized to mutate driver policy.
- The UI is unprivileged. Administrative actions are delegated through the installer or an explicit elevation flow, never by weakening pipe or device ACLs.

## 3. Components

### 3.1 Device-specific KMDF keyboard/HID filter driver

#### Goal

Suppress or rewrite selected HID usages for supported Apple keyboards before the original input reaches the Windows input subsystem, while leaving every unrelated device and usage untouched.

#### Responsibilities

- Install as an upper or lower filter at the narrowest technically valid HID/keyboard stack location for each supported transport.
- Bind through exact, reviewed INF hardware/compatible IDs and verify Apple vendor identity, model/collection identity, transport, HID descriptor fingerprint, and expected report layout at runtime.
- Expose a restricted device interface for service discovery, capabilities, health counters, and policy IOCTLs.
- Apply only bounded, declarative transformations: pass, suppress, or replace allowlisted usages/bit fields. The MVP MUST NOT execute scripts, parse arbitrary expressions, access files/network, or allocate on the input hot path.
- Keep policy immutable while active and replace it atomically.
- Preserve unrelated report IDs, usages, modifiers, rollover state, output/feature reports, and power/PnP operations.
- Detect device removal, surprise removal, suspend/resume, service restart, and stale policy generation safely.
- Emit rate-limited ETW/WPP diagnostics that contain device/model identifiers and counters, not typed text.

#### Non-goals

- Filtering all keyboards through a keyboard-class-wide registration.
- Device discovery UI, profile storage, model naming, update checks, or general macro execution.
- Implementing Apple Touch ID, biometric transport, Windows Hello, firmware update, or pairing.
- Depending on Raw Input, low-level hooks, or `SendInput` for production suppression.
- Rewriting a usage into a report collection that the physical device cannot represent. Such mappings require a separately designed virtual HID path and are post-MVP.

#### Interfaces and dependencies

- Windows Driver Frameworks (KMDF), WDM/HID/keyboard stack contracts, PnP, power management, and ETW/WPP.
- A release-signed INF/CAT/SYS package accepted by the supported Windows code-integrity policy.
- A versioned, fixed-width IOCTL ABI shared with the service. Every request includes total size, ABI major/minor, operation, device-instance token, descriptor fingerprint, policy generation, and payload count.
- Adapter-produced policy operations. The driver independently checks all offsets, masks, values, report IDs, lengths, duplicate/conflicting operations, and maximum counts.
- Driver device-interface ACL permitting read-only diagnostics as explicitly designed and policy mutation only to `SYSTEM`/the service identity and administrators.

#### Functional acceptance criteria

- On every supported model/transport, a mapped source usage is absent from downstream input and the configured replacement is observed exactly once.
- Unmapped keys, media usages, modifiers, rollover combinations, repeat, lock states, and output reports match the baseline behavior.
- An unrelated USB, Bluetooth, built-in, remote-session, and virtual keyboard shows byte-for-byte/equivalent behavior with the product installed and active.
- Unsupported descriptor fingerprints and unsupported transports receive no policy and pass input through unchanged.
- Policy replacement takes effect atomically: no report can observe a partially written policy.
- With the service and UI stopped, the last valid in-kernel policy remains active until device removal, reboot, or explicit policy clear, according to the documented lifecycle.
- Device reconnect and resume restore a valid policy through service reconciliation without duplicate input or a stuck key.

#### Reliability and security criteria

- No unbounded loops, recursion, pageable access at invalid IRQL, waiting, user-mode callbacks, or dynamic allocation on the report completion hot path.
- All integer arithmetic, buffer lengths, report offsets, and counts are checked before access; malformed reports and IOCTLs cannot cause out-of-bounds access or state corruption.
- Driver Verifier passes the configured KMDF, I/O, pool, deadlock, security, miscellaneous, and power checks for the release candidate.
- If initialization, identity validation, policy validation, or report parsing fails, the driver passes input through unchanged and records a bounded diagnostic.
- The INF cannot bind to a generic keyboard/HID class identifier alone. Each release includes a machine-reviewed binding diff and unrelated-device negative tests.
- Device ACLs, IOCTL authorization, and service identity prevent an unprivileged process from installing, replacing, clearing, or reading sensitive policy/state.
- Diagnostics never log raw keyboard reports in production mode.

#### Test criteria

- Unit-test report bounds, policy validation, descriptor matching, generation changes, and transformation logic using captured and synthetic fixtures.
- Fuzz all IOCTL decoders, policy validators, and report parsers with valid and malformed lengths, IDs, counts, and concurrency schedules.
- Run KMDF/Static Driver Verifier, CodeQL or equivalent static analysis, Driver Verifier soak tests, HLK tests applicable to the chosen stack position, and code-signing verification.
- Exercise boot, shutdown, rapid plug/unplug, Bluetooth disconnect, surprise removal, sleep/hibernate/resume, service crash/restart, update/rollback, and 24-hour high-rate input.
- Physically verify each allowlisted descriptor fingerprint and perform negative tests with unrelated Apple and non-Apple keyboards.

### 3.2 Rust Windows background service

#### Goal

Own system policy outside the kernel: discover supported devices, validate and persist configuration, select adapters, reconcile driver state, and provide a secure local API to the UI.

#### Responsibilities

- Run under the Windows Service Control Manager with the minimum practical privileges and explicit recovery behavior.
- Enumerate supported filter device interfaces and respond to PnP arrival/removal and session changes.
- Resolve device metadata to one exact adapter; reject ambiguity instead of guessing.
- Load, migrate, validate, and atomically persist configuration.
- Compile normalized user mappings through the adapter and submit a device- and descriptor-bound policy to the driver.
- Reconcile desired and actual policy generations at service start, device arrival, resume, and config change.
- Expose version/capability, device inventory, mapping CRUD, validation, apply status, health, and opt-in diagnostics over secured local IPC.
- Maintain bounded structured logs and Windows Event Log health events without recording typed content.

#### Non-goals

- Capturing global keystrokes, implementing a low-level hook, drawing UI, or handling every key report in user mode.
- Downloading or executing unsigned code, silently elevating the UI, or weakening driver security.
- Treating a friendly model name as sufficient device identity.
- Cloud accounts, remote control, analytics, or automatic upload of diagnostics.

#### Interfaces and dependencies

- Rust stable toolchain with pinned dependencies; Windows APIs for services, SetupAPI/Configuration Manager, named pipes, DPAPI or Windows credential protection where needed, and Event Log/ETW.
- Versioned named-pipe IPC and persisted config contracts defined in section 3.3.
- Versioned driver IOCTL ABI and adapter interface.
- Service data directories with ACLs separated into machine-wide state, per-user preferences, and non-sensitive logs.
- Installer-managed service identity, privileges, binary path, recovery policy, and upgrades.

#### Functional acceptance criteria

- Service startup discovers all connected supported devices and converges each to the latest valid desired policy without launching the UI.
- A valid configuration update is durably committed, applied, and acknowledged with config and policy generations; an invalid update changes neither disk nor driver state.
- Concurrent clients cannot lose updates: mutating requests use an expected revision or return a conflict.
- Device arrival/removal, Bluetooth reconnect, suspend/resume, and service restart converge idempotently without duplicate policies.
- An unsupported/ambiguous device is reported with a reason and remains pass-through.
- Stopping or crashing the service does not stop keyboard input.

#### Reliability and security criteria

- Parsing and validation occur before side effects. Config writes use write-new, flush, atomic replace, and recoverable backup semantics.
- Requests have length, nesting, collection-size, and time limits; stalled clients cannot exhaust service threads or handles.
- Pipe ACLs and client impersonation/authorization distinguish read-only status from per-user or administrative mutation. Session identity supplied in a payload is never trusted.
- The service uses no blanket debug, TCB, or unrestricted device privileges. Required privileges are documented and tested.
- Dependency versions are locked, audited, and reproducibly built. Unsafe Rust is isolated, justified, reviewed, and covered by tests.
- Crash recovery is bounded to avoid restart loops; repeated policy failure leaves that device pass-through and emits an actionable health state.

#### Test criteria

- Unit-test config migration, authorization decisions, policy reconciliation, atomic persistence, adapter selection, and driver error mapping.
- Integration-test IPC with real Windows named pipes and a simulated driver/device backend.
- Fault-inject truncated files, disk-full/access-denied writes, stale revisions, malformed messages, service termination between persistence steps, driver timeouts, and device removal during apply.
- Fuzz IPC/config decoders and adapter inputs; run Rust formatting, linting, dependency/license audit, and vulnerability scanning.
- Soak test repeated reconnect/resume/config updates and verify bounded handles, memory, CPU, log size, and recovery count.

### 3.3 Versioned IPC and configuration contract

#### Goal

Provide stable, evolvable, testable contracts among UI, service, adapter policy, persisted state, and driver without silent semantic changes.

#### Responsibilities

- Define canonical schemas, compatibility rules, limits, error codes, and migration behavior.
- Use a local named pipe such as `\\.\pipe\MagicKeyboard\v1` for UI/service communication. Messages are length-prefixed and carry protocol major/minor, message type, request ID, body length, and optional capability set.
- Define request/response correlation, cancellation/timeouts, server events, revision-based writes, and structured errors.
- Define a human-inspectable persisted configuration with an explicit `schemaVersion`, stable device selector, mapping list, defaults, and provenance. JSON is acceptable for persistence; the wire encoding MAY be CBOR or another deterministic bounded encoding.
- Distinguish four versions: UI/service protocol, config schema, adapter normalized-policy schema, and driver ABI. They MUST NOT be treated as one version number.
- Publish hard limits for message bytes, string bytes, mappings per device, devices, nesting, and diagnostics payloads.
- Preserve unknown fields only where the schema explicitly allows it; never reinterpret an existing field incompatibly.

#### Non-goals

- A public network API, remote-control protocol, executable configuration language, or storage of raw HID captures in normal config.
- Exposing raw driver IOCTL structures to the UI.
- Automatic downgrade of a newer config into a format that could lose meaning.

#### Interfaces and dependencies

- Serde-compatible Rust schema types and generated or shared TypeScript bindings for the Tauri frontend.
- JSON Schema or equivalent machine-readable validation for persisted config and checked fixtures for all wire messages.
- Driver ABI headers generated from, or mechanically checked against, a single fixed-layout definition with compile-time size/offset assertions in Rust and C.
- Named-pipe ACLs and service authorization policy; UTF-8 and canonical identifier rules.

#### Functional acceptance criteria

- Same-major peers negotiate capabilities; unsupported optional fields/features produce a defined capability error rather than a crash.
- A protocol-major mismatch is rejected before processing the body and reports both supported ranges.
- Every mutating response returns the resulting config revision and per-device apply generation/status.
- Every released config version migrates forward through deterministic, idempotent migrations; migration failure preserves the original file.
- Older compatible clients can read status and edit fields they understand without silently deleting protected newer fields.
- Golden fixtures decode identically in Rust and TypeScript, and fixed driver ABI layouts match on every supported architecture.

#### Reliability and security criteria

- Decoders reject oversized, truncated, duplicate-critical-field, invalid UTF-8, excessive nesting, integer-overflow, and trailing-data inputs.
- Secrets are not part of the contract unless a later threat model requires them. If introduced, secrets are referenced, not echoed, and protected with Windows facilities.
- Error messages contain safe identifiers and field paths, not raw report bytes, typed content, access tokens, or filesystem secrets.
- Config selectors cannot broaden driver binding; only driver/installer release artifacts can add supported hardware identities.
- Contract changes require compatibility notes, fixture changes, migration tests, and security review.

#### Test criteria

- Maintain golden valid/invalid fixtures for every released protocol/config/ABI version.
- Property-test encode/decode round trips and fuzz all decoders and migrations.
- Run cross-version matrices for current UI with current/previous service and current service with current/previous config, subject to the published support window.
- Test 32/64-bit layout assumptions even when the MVP ships x64; use fixed-width fields and prohibit pointer-sized ABI data.
- Test downgrade refusal, unknown fields, duplicate requests, retries, timeouts, cancellation, stale revisions, and event backpressure.

### 3.4 Apple model and transport HID adapter layer

#### Goal

Convert model- and transport-specific HID descriptors/reports into normalized key capabilities and compile user mappings into safe, descriptor-bound driver policy.

#### Responsibilities

- Represent each supported `(vendor, product/model, transport, HID collection, descriptor fingerprint)` as explicit adapter data and code.
- Parse descriptors and report layouts from captured fixtures; normalize physical keys/usages into stable internal key IDs independent of friendly names.
- Account for USB/Bluetooth differences in report IDs, collection layout, usage pages, offsets, rollover encoding, and optional keys.
- Expose capabilities to the service/UI, including which source and destination mappings are representable without a virtual device.
- Compile mappings into bounded report operations tied to the exact descriptor fingerprint and validate collisions, unreachable states, modifier semantics, and unsupported cross-collection mappings.
- Keep provenance for every model constant, descriptor fixture, and behavior claim.

#### Non-goals

- Guessing support based only on the word “Apple,” a product string, appearance, or a nearby product ID.
- Handling Touch ID/biometric reports, Apple secure-element protocols, firmware updates, or Windows Hello.
- Embedding UI labels, installer binding policy, or arbitrary kernel code generation.
- Claiming that USB and Bluetooth layouts are equivalent without separate fixtures and hardware tests.

#### Interfaces and dependencies

- Service-facing adapter trait: `match_device`, `describe_capabilities`, `normalize_config`, `compile_policy`, and `explain_unsupported`.
- Immutable HID descriptor/report fixtures produced by the recorder/probe and reviewed metadata containing VID/PID, transport, collection IDs, descriptor hash, model label, source/provenance, and capture tool version.
- Normalized versioned model schema and driver policy schema.
- HID Usage Tables and public Windows HID documentation; independently observed device behavior under the clean-room rules below.

#### Functional acceptance criteria

- Exact supported fixtures select one and only one adapter; unknown or altered fixtures select none.
- USB and Bluetooth instances of a supported model expose intentionally equivalent normalized key IDs where hardware behavior is equivalent.
- For each advertised mapping, compilation emits a policy that transforms the expected report field and no unrelated field.
- Unsupported targets, ambiguous chords, conflicting mappings, out-of-range offsets, and cross-collection operations return actionable validation errors.
- Capability data shown by the UI comes from the selected adapter and never advertises an untestable mapping.
- Adding an adapter does not alter golden outputs for existing adapters unless accompanied by an explicit contract change.

#### Reliability and security criteria

- Descriptor/report parsing is total and bounds-checked; unknown data returns unsupported, never a guessed layout.
- Adapter output is treated as untrusted by the driver and remains within documented operation/count limits.
- No fixture contains real typed text, biometric data, pairing secrets, link keys, serial numbers that are not necessary, or other personal data.
- Model matching uses multiple independently validated signals and exact fingerprints. Friendly strings are display-only.
- Adapter provenance satisfies the legal clean-room requirements in section 6.

#### Test criteria

- Golden-test matching, capability enumeration, normalization, and compiled policy for every fixture.
- Differential-test USB/Bluetooth normalized behavior where both transports are supported.
- Property-test operation bounds/non-overlap and fuzz descriptor, report, and mapping inputs.
- Replay synthetic and consented reports through adapter plus driver transformation logic and assert source suppression, replacement, rollover, and unrelated-bit preservation.
- Require physical qualification for every new fingerprint; fixtures alone cannot promote a model to supported.

### 3.5 HID report recorder/probe

#### Goal

Provide a developer-only, consent-driven tool to identify HID topology, capture descriptors, and record minimal report transitions needed to build and verify adapters.

#### Responsibilities

- Enumerate HID devices/collections with stable SetupAPI paths, VID/PID, transport, usage page/usage, descriptor information, and access status.
- Support multiple acquisition backends because ordinary user-mode access to keyboard collections may be denied or exclusive: read-only descriptor probing, Raw Input device-correlated event diagnostics, and an explicitly enabled filter diagnostic capture path.
- Clearly label Raw Input output as observational: it can identify devices for diagnostics but cannot suppress system input.
- Record timestamped report bytes or normalized transitions only during an explicit, time-bounded capture session with a visible privacy warning.
- Sanitize exports, attach capture metadata/provenance, and produce deterministic fixtures.
- Compare two captures and highlight report IDs/bit changes without assigning undocumented semantics automatically.

#### Non-goals

- A background keylogger, production input path, remapping backend, cloud uploader, or way to bypass Windows HID access controls.
- Capturing Touch ID, biometric, secure-element, pairing, or authentication traffic.
- Inferring a complete model specification from one key press or silently adding a device to the production allowlist.

#### Interfaces and dependencies

- Rust CLI/library using SetupAPI/Configuration Manager, HID APIs, optional Raw Input diagnostic window, and a separately authorized filter trace interface.
- Adapter fixture format and provenance manifest.
- Explicit command-line output path; restrictive file ACLs; optional redaction mode; no network dependency.
- Development/test-signed driver capture features that are compiled out or cryptographically disabled in production releases unless separately approved.

#### Functional acceptance criteria

- Probe output uniquely identifies the physical device path, collection, transport, descriptor hash, and whether raw report access is available.
- A capture starts only after explicit confirmation, displays duration/status, stops on timeout/device removal/Ctrl+C, and closes all handles.
- The tool can record a controlled single-key test and emit a deterministic sanitized fixture with tool version and capture metadata.
- If Windows denies direct keyboard HID access, the tool reports that limitation and available authorized alternatives; it does not claim no device exists.
- Raw Input diagnostics correlate events to a device but make no suppression guarantee.

#### Reliability and security criteria

- Default mode captures descriptors/metadata only. Raw report capture requires an explicit high-friction flag and privacy confirmation.
- Captures are local-only, bounded by size/time, created with restrictive ACLs, and never auto-uploaded.
- Production logs and crash reports exclude report bytes. Export requires an explicit path and warns that keyboard reports may reveal typed content.
- The tool cannot enable a broad filter, modify driver policy, or access biometric collections.
- Parsing and comparison tolerate malformed/truncated reports without panics or out-of-bounds access.

#### Test criteria

- Unit-test sanitization, descriptor hashing, bounds, timeout, fixture determinism, and metadata provenance.
- Integration-test direct-access success, access denied, device removal, malformed reports, output failure, and cancellation.
- Verify with two simultaneous keyboards that Raw Input attribution is correct while documenting that suppression is impossible.
- Privacy-test defaults, ACLs, redaction, size/time limits, log output, and production-build feature exclusion.
- Validate emitted fixtures through the adapter golden-test suite before accepting them.

### 3.6 Tauri settings and tray UI

#### Goal

Let a standard user understand device support, configure valid mappings, inspect apply status, and control tray behavior without handling input or requiring continuous UI execution.

#### Responsibilities

- Display service/driver versions, connected devices, model/transport, support reason, active config revision, and apply health.
- Render only adapter-advertised keys and valid destinations; validate locally for fast feedback and rely on authoritative service validation.
- Support mapping add/edit/remove, reset, conflict display, save/apply status, and clear recovery guidance.
- Provide tray access, launch-at-login preference, service health notifications, and links to local diagnostics/export actions.
- Use Tauri commands as a narrow bridge to a typed IPC client; keep privileged operations out of web content.
- Meet keyboard navigation, screen-reader, scaling, high-contrast, and localization-ready layout requirements.

#### Non-goals

- Capturing keystrokes, opening the driver, running elevated for routine use, interpreting HID reports, or remaining open for mappings to work.
- Embedding remote web content, third-party advertisements, cloud accounts, telemetry by default, or auto-uploading captures.
- Presenting Touch ID as configurable or suggesting Windows Hello support.

#### Interfaces and dependencies

- Tauri desktop shell, pinned Rust backend dependencies, and a minimal frontend bundle with a restrictive Content Security Policy.
- Generated/shared IPC types and service named-pipe client.
- Service events for device/status changes with bounded reconnect/backoff.
- Installer-owned shortcuts and optional per-user launch registration.

#### Functional acceptance criteria

- A user can identify a supported device, create a valid mapping, apply it, see the resulting revisions/status, and verify persistence after UI and service restart.
- Invalid/conflicting/unsupported mappings cannot be submitted without a clear field-level explanation; server rejection remains authoritative and is displayed.
- Unsupported devices show pass-through status and a precise non-support reason without offering mappings.
- Closing or crashing the UI has no effect on active keyboard behavior.
- UI/service version incompatibility produces an explicit upgrade message rather than an empty or partially editable screen.
- Tray and full settings window remain usable at supported DPI/scaling and with keyboard-only navigation.

#### Reliability and security criteria

- No remote navigation, arbitrary URL execution, devtools, or broad Tauri allowlists in production. CSP permits only packaged resources required by the app.
- IPC responses and device strings are rendered as data, never executable HTML. External links require explicit user action and use an allowlist.
- The UI runs unelevated and stores no privileged credentials. Administrative operations use a signed installer/elevation boundary.
- Reconnect uses bounded exponential backoff; event queues are coalesced so device storms cannot freeze the UI.
- Frontend/backend dependency, license, and vulnerability scans pass release policy.

#### Test criteria

- Unit/component-test mapping editing, capability gating, conflict messages, status transitions, and version errors.
- End-to-end test against a simulated service for first run, save/apply, stale revision, service unavailable/restart, device arrival/removal, and unsupported device.
- Accessibility-test keyboard traversal, focus visibility, screen-reader names, high contrast, 100–200% scaling, and reduced motion where applicable.
- Security-test CSP, Tauri command allowlist, untrusted strings, external links, packaged-resource integrity, and absence of dev endpoints.
- Smoke-test UI launch/exit/tray behavior on a clean supported Windows image; verify remapping continues after UI exit.

### 3.7 Installer and updater

#### Goal

Install, upgrade, repair, and remove the signed driver, service, UI, and configuration safely, with clear compatibility checks and rollback/recovery.

#### Responsibilities

- Package exact driver INF/CAT/SYS, service, UI, contracts, licenses, and support-matrix metadata as one coherent release.
- Verify supported Windows version/architecture, administrative rights, signature chain, Secure Boot/code-integrity compatibility, and conflicting versions before mutation.
- Install the driver package in the Driver Store, register only approved device filters, register/configure the service, and install the per-user/all-user UI as designed.
- Order upgrades so service, contract, and driver ABI remain compatible; quiesce policy changes and handle reboot-required states explicitly.
- Verify signed update metadata and payload hashes, stage atomically, retain one known-good rollback path where Windows driver servicing permits, and never self-update from unsigned metadata.
- Support repair and uninstall, including filter detachment, service removal, binary cleanup, and an explicit choice to retain/delete user configuration.

#### Non-goals

- Bypassing driver-signing enforcement, disabling Secure Boot, using test signing on production systems, or modifying unrelated keyboard class settings.
- Silent major-version config downgrade, third-party bundle installation, or a mandatory always-running network updater.
- Deleting user data without explicit policy/consent or claiming no reboot when Windows requires one.

#### Interfaces and dependencies

- WiX/MSI plus bootstrapper or an equivalent Windows installer technology that correctly supports driver packages and transactional application installation.
- Microsoft driver signing/attestation or required certification pipeline, Authenticode signing, timestamping, and protected signing keys.
- Service Control Manager, PnPUtil/SetupAPI-compatible driver servicing APIs, Windows Restart Manager where applicable, and signed update manifest format.
- Release manifest declaring component versions, protocol/config/ABI ranges, OS/architecture matrix, file hashes, signatures, and rollback compatibility.

#### Functional acceptance criteria

- A clean install on every supported Windows target produces a running service, correctly bound filter on allowlisted hardware only, launchable UI, and working baseline mapping after any declared reboot.
- Upgrade from every supported prior release preserves/migrates config and restores policy without broadening device binding.
- Repair restores missing/corrupt packaged files and registrations without replacing valid user configuration.
- Uninstall removes the device filter and service without disabling any keyboard; after required restart/re-enumeration, all keyboards use the standard Windows path.
- Unsupported OS/architecture, invalid signature, incompatible downgrade, or insufficient privilege fails before partial installation and gives an actionable message.
- Update failure leaves either the previous coherent release or a clearly recoverable pass-through state, never a mixed ABI that blocks keyboard input.

#### Reliability and security criteria

- All executable, driver, catalog, installer, manifest, and update artifacts are signed and verified before execution/install.
- Signing keys are isolated in approved hardware-backed CI/signing infrastructure; keys and secrets are absent from source, logs, and build artifacts.
- Installer custom actions are minimal, rollback-aware, quoted against path injection, and never load code from user-writable locations.
- Update transport uses TLS plus artifact signature verification; TLS alone is insufficient. Rollback protection prevents installation of known-vulnerable releases unless explicitly authorized for recovery.
- Driver binding is diffed and negative-tested on each release. Uninstall/rollback has a tested recovery procedure using standard Windows recovery tools.

#### Test criteria

- Test clean install, passive/interactive install, repair, upgrade, downgrade refusal, rollback, uninstall, reboot-required flow, offline mode, and interrupted power/process scenarios in disposable VMs.
- Test with supported hardware present/absent and unrelated keyboards connected; verify only allowlisted device instances receive the filter.
- Verify signatures, timestamps, hashes, Driver Store state, service ACL/config, file ACLs, uninstall records, and no test certificates/features.
- Fault-inject locked files, service failure, driver staging failure, corrupt config, invalid manifest, disk full, and network loss.
- Run post-uninstall physical keyboard tests and a documented Windows Recovery Environment recovery drill before release.

### 3.8 Automated and hardware test strategy

#### Goal

Demonstrate that remapping is correct and device-specific, unrelated input remains unchanged, failure is pass-through, and release artifacts are secure and recoverable.

#### Responsibilities

- Maintain test layers: pure unit/property tests, fuzzing, contract/golden tests, simulated integration, driver/VM tests, installer tests, and physical hardware qualification.
- Maintain a versioned hardware matrix keyed by exact model number, VID/PID, transport, HID collection, descriptor fingerprint, keyboard layout, Windows build, architecture, firmware if observable, and test result.
- Provide a simulated HID/device backend for deterministic CI and an isolated Windows driver lane for privileged tests.
- Define baseline traces and assertions for key down/up, modifiers, rollover, repeat, lock keys/LEDs, media keys, reconnect, sleep/resume, and mapping changes.
- Gate allowlist changes and releases on negative-device, security, performance, upgrade, rollback, and uninstall results.
- Quarantine flaky hardware tests only with owner, reason, expiry, and a release-impact decision.

#### Non-goals

- Treating emulator/fixture success as physical-device support.
- Shipping a model based on a single manual smoke test.
- Capturing ordinary employee/user typing, biometric data, or undocumented third-party test data.
- Using production signing credentials in pull-request CI.

#### Interfaces and dependencies

- Rust/C/C++/TypeScript test frameworks as applicable, Windows VMs, Driver Verifier, Static Driver Verifier/CodeQL-equivalent analysis, fuzzers, and installer test images.
- Hardware lab with remotely controllable USB power where feasible, Bluetooth reset/re-pair capability, system sleep/wake control, and an independent observation method.
- Sanitized HID fixtures and expected normalized/driver output traces.
- CI artifact manifests, test reports, crash dumps with privacy scrubbing, and release support matrix.

#### Functional acceptance criteria

- CI rejects a change that alters an existing adapter golden result, contract fixture, ABI layout, INF binding, or installer manifest without an explicitly reviewed expectation update.
- Every advertised model/transport has a passing current physical qualification; every exact INF binding has both a positive target test and unrelated-device negative tests.
- End-to-end tests prove the source usage is suppressed and the replacement appears exactly once; they do not infer success merely from UI status.
- Simulated and hardware tests cover simultaneous supported and unsupported keyboards and verify mappings affect only the selected physical device.
- Release qualification covers clean install, boot, reconnect, sleep/resume, service/UI failure, upgrade, rollback, and uninstall.

#### Reliability and security criteria

- Release lanes have documented retry policy and preserve first-failure evidence; repeated retries cannot convert a deterministic failure into a pass.
- Test fixtures are reviewed for typed content, biometric data, serial numbers, secrets, and license/provenance before commit.
- Privileged test hosts are isolated, restored to known state, and use test certificates only; production signing is a separate gated process.
- Performance gates measure filter-path latency, CPU, memory, handle/pool growth, dropped/duplicate reports, and log volume under rollover and reconnect stress.
- Security gates include dependency audit, static analysis, parser/IOCTL fuzzing, ACL verification, signature verification, and recovery testing.

#### Test criteria

- Required per pull request: unit, property, contract/golden, static analysis, lint, dependency policy, simulated integration, and changed-parser fuzz smoke corpus.
- Required on protected scheduled lanes: Driver Verifier, extended fuzzing, suspend/reconnect soak, installer lifecycle, and supported Windows build matrix.
- Required for release: full physical matrix, unrelated-device negative matrix, 24-hour input/reconnect soak, performance thresholds, signed-artifact verification, upgrade from supported prior versions, rollback, uninstall, and recovery drill.
- A failed or missing required cell blocks support/release; it cannot be waived without a documented risk acceptance, owner, scope, and expiry.

## 4. MVP and later phases

### MVP

- Windows 11 x64 on explicitly listed builds; additional Windows versions/architectures are unsupported until added to the tested release matrix.
- One or more explicitly allowlisted Apple Magic Keyboard model/transport/descriptor combinations. USB and Bluetooth are qualified separately; support for one does not imply the other.
- Signed, narrowly device-bound KMDF filter with pass/suppress/replace transformations representable in the existing HID collection.
- Rust service with atomic machine configuration, exact adapter selection, policy reconciliation, health status, and secured local IPC.
- Versioned protocol/config/adapter/driver contracts with forward migrations and golden fixtures.
- Tauri settings/tray UI for device status and simple one-to-one mappings.
- Developer recorder/probe with descriptor-first, consent-based capture.
- Signed install, repair, upgrade, rollback/recovery, and uninstall path.
- Automated CI plus physical qualification for every advertised support-matrix entry.

### Later phases

- Additional Apple models, descriptor revisions, keyboard layouts, Bluetooth variants, supported Windows builds, and ARM64, each independently qualified.
- Per-user or per-application profiles after session/identity and focus spoofing risks are designed and tested.
- Chords, layers, macros, or cross-collection destinations only after explicit UX, accessibility, abuse, stuck-key, timing, and synthetic-input threat models.
- A separately signed virtual HID companion only if required for mappings the physical report collections cannot represent; it is not implied by the MVP filter.
- Enterprise policy/managed deployment, richer diagnostics, localization, and optional privacy-preserving update notifications.

Phase advancement MUST preserve exact binding, pass-through failure, compatibility, privacy, signing, and physical qualification requirements.

## 5. Explicit exclusions

- **Touch ID and Windows Hello are out of scope.** The product MUST NOT read, proxy, transform, store, advertise, or attempt to register Touch ID/biometric data; emulate a Windows Hello authenticator; access Apple secure-element traffic; or claim that an Apple Touch ID sensor works as a Windows biometric device.
- The ordinary keyboard keys on a Touch ID-equipped model MAY be supported only when the keyboard HID collections are independently identified and tested and all biometric collections are excluded and untouched.
- No Apple firmware updates, device pairing replacement, battery firmware management, boot-key support claims, or macOS behavior emulation beyond documented key mappings.
- No generic all-keyboard remapper, keylogger, remote administration tool, cloud account, telemetry by default, or capture upload.
- No production Raw Input or low-level-hook remapping backend, broad class filter, unsigned/test-signed production driver, Secure Boot bypass, or Windows security-policy weakening.
- No guarantee for devices, descriptor revisions, transports, Windows builds, architectures, remote sessions, or virtualization environments absent from the published support matrix.

## 6. Legal and clean-room warning

Apple device protocols and identifiers may be protected by copyright, patent, trademark, contract, anti-circumvention, or other law. This engineering specification is not legal advice. Counsel MUST review distribution names, reverse-engineering methods, captured artifacts, third-party licenses, driver behavior, and target jurisdictions before public release.

Implementation MUST use a documented clean-room process:

- Use public HID/Windows specifications and observations from lawfully acquired devices operated by authorized testers.
- Do not copy, disassemble, decompile, extract, redistribute, or depend on Apple binaries, firmware, drivers, confidential/leaked documents, cryptographic material, biometric data, or access-control circumvention.
- Keep provenance for each VID/PID, descriptor fixture, report interpretation, model claim, and external reference, including who observed it, with which tool/device, under what authorization, and when.
- Store only the minimum sanitized capture necessary to reproduce keyboard behavior. Never collect Touch ID, secure-element, pairing, authentication, or ordinary typing data.
- Require contributor attestation that adapter data is independently derived from permitted sources. Quarantine data with unknown or incompatible provenance.
- Use Apple and Magic Keyboard names only as legally reviewed nominative compatibility references; do not imply endorsement, certification, or affiliation.

If provenance or legal permission is uncertain, the affected model/feature MUST remain unsupported until reviewed.

## 7. Definition of Done

The MVP is done only when all of the following are true:

- Product scope, support matrix, threat model, privacy model, driver binding location, ABI/protocol compatibility policy, and recovery procedure are reviewed and versioned.
- Every component above meets its functional, reliability/security, and test criteria; no required release gate is missing, waived indefinitely, or represented only by a plan.
- All advertised model/transport/descriptor entries pass physical end-to-end qualification, and unrelated-device negative tests prove the filter does not affect other keyboards.
- Raw Input and low-level hooks are absent from the production remapping path; the signed device-bound filter suppresses each mapped source and emits the replacement exactly once.
- The driver passes static analysis, applicable HLK checks, Driver Verifier, parser/IOCTL fuzzing, power/PnP stress, latency/resource thresholds, and signed-package verification.
- Service, IPC/config migrations, adapter fixtures, UI, installer, updater, and test tooling pass their automated, fault-injection, privacy, accessibility, and security gates.
- Clean install, reboot, reconnect, sleep/resume, service/UI crash, repair, supported upgrade, failed update recovery, rollback, uninstall, and post-uninstall keyboard operation pass on clean supported Windows images.
- Failure of UI, service, config, adapter match, policy validation, or update leaves keyboard input available; unsupported devices remain pass-through.
- Production artifacts are reproducibly associated with reviewed source, dependency lockfiles/SBOM, licenses, hashes, signatures/timestamps, release manifest, and test evidence. No test certificates, capture features, debug endpoints, secrets, or raw keyboard data are present.
- Touch ID/Windows Hello exclusions are verified in code, UI, driver binding, diagnostics, tests, documentation, and release claims.
- Legal/clean-room provenance and trademark/license review are complete for every shipped model fixture and external dependency.
- User-facing installation, recovery, privacy, support-matrix, known-limitations, and uninstall documentation is complete and matches tested behavior.
