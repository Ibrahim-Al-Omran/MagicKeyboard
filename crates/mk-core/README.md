# mk-core

`mk-core` contains the platform-neutral contract for MagicKeyboard:

- schema-versioned JSON configuration;
- stable device identity and transport selectors;
- normalized HID-usage input events;
- mapping triggers and action intents;
- deterministic profile/rule selection;
- validation and same-directory temporary-file persistence.

The crate never opens a device or executes an action. Native adapters belong in
the service layer. Schema version `1` is the only version currently accepted;
future incompatible model changes must increment `schema_version` and add an
explicit migration path.
