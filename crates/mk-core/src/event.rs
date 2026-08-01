use serde::{Deserialize, Serialize};

use crate::DeviceId;

/// A control identified by its USB HID usage page and usage.
///
/// Adapters for non-HID transports should translate native controls into this
/// same namespace so mappings remain portable.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct ControlCode {
    pub usage_page: u16,
    pub usage: u16,
}

impl ControlCode {
    pub const KEYBOARD_PAGE: u16 = 0x07;
    pub const CONSUMER_PAGE: u16 = 0x0c;
    pub const BUTTON_PAGE: u16 = 0x09;

    pub const fn new(usage_page: u16, usage: u16) -> Self {
        Self { usage_page, usage }
    }

    pub const fn keyboard(usage: u16) -> Self {
        Self::new(Self::KEYBOARD_PAGE, usage)
    }

    pub const fn consumer(usage: u16) -> Self {
        Self::new(Self::CONSUMER_PAGE, usage)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonState {
    Pressed,
    Released,
}

/// Input data after transport-specific decoding and normalization.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputEventKind {
    Button {
        control: ControlCode,
        state: ButtonState,
        #[serde(default)]
        repeat: bool,
    },
    Axis {
        control: ControlCode,
        value: i32,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InputEvent {
    pub device_id: DeviceId,
    /// A source-local monotonic sequence number.
    pub sequence: u64,
    /// Monotonic time supplied by the source, in microseconds.
    pub timestamp_micros: u64,
    #[serde(flatten)]
    pub event: InputEventKind,
}

impl InputEvent {
    pub fn button(
        device_id: DeviceId,
        sequence: u64,
        timestamp_micros: u64,
        control: ControlCode,
        state: ButtonState,
    ) -> Self {
        Self {
            device_id,
            sequence,
            timestamp_micros,
            event: InputEventKind::Button {
                control,
                state,
                repeat: false,
            },
        }
    }
}
