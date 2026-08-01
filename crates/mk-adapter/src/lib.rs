//! Fail-closed selection of qualified Apple keyboard adapters.
//!
//! A device is supported only when its vendor/product identity, transport, and
//! complete HID report-descriptor fingerprint match one registry entry. The
//! production registry is deliberately empty until captured hardware has been
//! qualified.

#![forbid(unsafe_code)]

mod capability;
mod fingerprint;
mod policy;
mod registry;

pub use capability::{CapabilityError, NormalizedCapabilities, MAX_NORMALIZED_CONTROLS};
pub use fingerprint::{DescriptorFingerprint, ParseFingerprintError};
pub use mk_core::ControlCode;
pub use policy::{
    compile_policy, CompilePolicyError, DriverOperation, DriverPolicy, OneToOneMapping,
    PolicyLimitError, PolicyLimits, MAX_DRIVER_POLICY_OPERATIONS,
};
pub use registry::{
    built_in_registry, AdapterDefinition, AdapterTransport, DeviceDescriptor, SelectionError,
    SupportRegistry, APPLE_VENDOR_ID,
};

#[cfg(test)]
mod tests;
