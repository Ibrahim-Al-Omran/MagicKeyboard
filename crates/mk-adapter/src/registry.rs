use std::error::Error;
use std::fmt;

use mk_core::Transport;

use crate::{DescriptorFingerprint, NormalizedCapabilities};

/// Apple's USB vendor identifier. No product identifier is built into this crate.
pub const APPLE_VENDOR_ID: u16 = 0x05ac;

/// Transport is part of adapter identity and is never inferred.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AdapterTransport {
    Usb,
    BluetoothClassic,
    BluetoothLowEnergy,
}

impl AdapterTransport {
    /// Normalize only transports with exact adapter semantics.
    ///
    /// `Unknown`, virtual, and vendor-specific transports intentionally return
    /// `None` so callers cannot silently classify them.
    pub fn from_core(transport: &Transport) -> Option<Self> {
        match transport {
            Transport::Usb => Some(Self::Usb),
            Transport::Bluetooth => Some(Self::BluetoothClassic),
            Transport::BluetoothLowEnergy => Some(Self::BluetoothLowEnergy),
            Transport::Virtual | Transport::Unknown | Transport::Other(_) => None,
        }
    }
}

/// Identity and complete descriptor bytes observed by a probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceDescriptor<'a> {
    pub vendor_id: u16,
    pub product_id: u16,
    pub transport: AdapterTransport,
    pub report_descriptor: &'a [u8],
}

impl DeviceDescriptor<'_> {
    pub fn fingerprint(&self) -> DescriptorFingerprint {
        DescriptorFingerprint::of(self.report_descriptor)
    }
}

/// One hardware-qualified model/transport/descriptor implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterDefinition {
    model: String,
    product_id: u16,
    transport: AdapterTransport,
    descriptor_fingerprint: DescriptorFingerprint,
    capabilities: NormalizedCapabilities,
}

impl AdapterDefinition {
    pub fn new(
        model: impl Into<String>,
        product_id: u16,
        transport: AdapterTransport,
        descriptor_fingerprint: DescriptorFingerprint,
        capabilities: NormalizedCapabilities,
    ) -> Self {
        Self {
            model: model.into(),
            product_id,
            transport,
            descriptor_fingerprint,
            capabilities,
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub const fn product_id(&self) -> u16 {
        self.product_id
    }

    pub const fn transport(&self) -> AdapterTransport {
        self.transport
    }

    pub const fn descriptor_fingerprint(&self) -> DescriptorFingerprint {
        self.descriptor_fingerprint
    }

    pub fn capabilities(&self) -> &NormalizedCapabilities {
        &self.capabilities
    }
}

/// Explicit support allowlist.
///
/// `Default` and `empty` contain no devices. Definitions must come from
/// reviewed captures and hardware qualification; this crate ships no product
/// IDs or real-device fingerprints.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SupportRegistry {
    definitions: Vec<AdapterDefinition>,
}

/// Return the shipped, hardware-qualified support registry.
///
/// It is intentionally empty: no real hardware captures have graduated.
pub fn built_in_registry() -> SupportRegistry {
    SupportRegistry::empty()
}

impl SupportRegistry {
    pub const fn empty() -> Self {
        Self {
            definitions: Vec::new(),
        }
    }

    pub fn from_qualified_definitions(
        definitions: impl IntoIterator<Item = AdapterDefinition>,
    ) -> Self {
        Self {
            definitions: definitions.into_iter().collect(),
        }
    }

    pub fn definitions(&self) -> &[AdapterDefinition] {
        &self.definitions
    }

    /// Select exactly one definition or fail closed.
    pub fn select(
        &self,
        descriptor: &DeviceDescriptor<'_>,
    ) -> Result<&AdapterDefinition, SelectionError> {
        if descriptor.vendor_id != APPLE_VENDOR_ID {
            return Err(SelectionError::NonAppleVendor {
                observed: descriptor.vendor_id,
            });
        }

        let identity_matches = self
            .definitions
            .iter()
            .filter(|definition| {
                definition.product_id == descriptor.product_id
                    && definition.transport == descriptor.transport
            })
            .collect::<Vec<_>>();

        if identity_matches.is_empty() {
            return Err(SelectionError::UnsupportedModelTransport {
                product_id: descriptor.product_id,
                transport: descriptor.transport,
            });
        }

        let observed_fingerprint = descriptor.fingerprint();
        let fingerprint_matches = identity_matches
            .into_iter()
            .filter(|definition| {
                definition.descriptor_fingerprint == observed_fingerprint
            })
            .collect::<Vec<_>>();

        match fingerprint_matches.as_slice() {
            [definition] => Ok(*definition),
            [] => Err(SelectionError::DescriptorFingerprintMismatch {
                product_id: descriptor.product_id,
                transport: descriptor.transport,
                observed: observed_fingerprint,
            }),
            matches => Err(SelectionError::AmbiguousAdapter {
                product_id: descriptor.product_id,
                transport: descriptor.transport,
                fingerprint: observed_fingerprint,
                match_count: matches.len(),
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionError {
    NonAppleVendor {
        observed: u16,
    },
    UnsupportedModelTransport {
        product_id: u16,
        transport: AdapterTransport,
    },
    DescriptorFingerprintMismatch {
        product_id: u16,
        transport: AdapterTransport,
        observed: DescriptorFingerprint,
    },
    AmbiguousAdapter {
        product_id: u16,
        transport: AdapterTransport,
        fingerprint: DescriptorFingerprint,
        match_count: usize,
    },
}

impl fmt::Display for SelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonAppleVendor { observed } => {
                write!(formatter, "vendor {observed:04x} is not Apple")
            }
            Self::UnsupportedModelTransport {
                product_id,
                transport,
            } => write!(
                formatter,
                "Apple product {product_id:04x} over {transport:?} is unsupported"
            ),
            Self::DescriptorFingerprintMismatch {
                product_id,
                transport,
                observed,
            } => write!(
                formatter,
                "descriptor {observed} is not qualified for Apple product {product_id:04x} over {transport:?}"
            ),
            Self::AmbiguousAdapter {
                product_id,
                transport,
                fingerprint,
                match_count,
            } => write!(
                formatter,
                "{match_count} adapters match Apple product {product_id:04x} over {transport:?} with descriptor {fingerprint}"
            ),
        }
    }
}

impl Error for SelectionError {}
