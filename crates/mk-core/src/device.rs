use std::fmt;

use serde::{Deserialize, Serialize};

/// Stable application identity for a physical or virtual input device.
#[derive(
    Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct DeviceId(pub String);

impl DeviceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Transport information normalized across platform-specific device APIs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Usb,
    Bluetooth,
    BluetoothLowEnergy,
    Virtual,
    Unknown,
    Other(String),
}

/// Hardware and user-facing identity supplied by a device adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeviceIdentity {
    pub id: DeviceId,
    pub vendor_id: u16,
    pub product_id: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub transport: Transport,
}

/// A portable predicate used to select a profile for a device.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeviceSelector {
    Id {
        id: DeviceId,
    },
    Hardware {
        vendor_id: u16,
        product_id: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        serial_number: Option<String>,
    },
    Transport {
        transport: Transport,
    },
}

impl DeviceSelector {
    pub fn matches(&self, device: &DeviceIdentity) -> bool {
        match self {
            Self::Id { id } => id == &device.id,
            Self::Hardware {
                vendor_id,
                product_id,
                serial_number,
            } => {
                vendor_id == &device.vendor_id
                    && product_id == &device.product_id
                    && match serial_number {
                        Some(serial) => device.serial_number.as_ref() == Some(serial),
                        None => true,
                    }
            }
            Self::Transport { transport } => transport == &device.transport,
        }
    }
}
