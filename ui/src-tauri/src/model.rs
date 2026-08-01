use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KeyboardModelId {
    MagicKeyboardTouchId,
    MagicKeyboardCompact,
    MagicKeyboardNumeric,
}

impl KeyboardModelId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MagicKeyboardTouchId => "magic-keyboard-touch-id",
            Self::MagicKeyboardCompact => "magic-keyboard-compact",
            Self::MagicKeyboardNumeric => "magic-keyboard-numeric",
        }
    }
}

impl Default for KeyboardModelId {
    fn default() -> Self {
        Self::MagicKeyboardTouchId
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BatteryState {
    pub percent: u8,
    pub charging: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionState {
    Connected,
    Connecting,
    Disconnected,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub id: String,
    pub name: String,
    pub model_id: String,
    pub connection: ConnectionState,
    pub battery: Option<BatteryState>,
    pub fn_lock: bool,
    pub active_profile_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MediaCommand {
    PlayPause,
    Previous,
    Next,
    VolumeUp,
    VolumeDown,
    Mute,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum KeyAction {
    Disabled,
    Key { key: String },
    Shortcut { keys: Vec<String> },
    Media { command: MediaCommand },
    Launch { target: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KeyMapping {
    pub source_key: String,
    pub action: KeyAction,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub mappings: Vec<KeyMapping>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub devices: Vec<DeviceStatus>,
    pub selected_device_id: Option<String>,
    pub profiles: Vec<Profile>,
    pub remapper_active: bool,
    pub status_message: String,
    pub launch_at_startup: bool,
    pub diagnostics: crate::remap::Diagnostics,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSettings {
    pub fn_lock: bool,
    #[serde(default = "default_true")]
    pub launch_at_startup: bool,
    pub active_profile_id: String,
    pub profiles: Vec<Profile>,
}

fn default_true() -> bool {
    true
}
