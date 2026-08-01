use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::PathBuf;

use crate::autostart;
use crate::detect::{self, DetectedKeyboard};
use crate::model::{
    AppSnapshot, ConnectionState, DeviceStatus, KeyAction, KeyMapping, MediaCommand,
    PersistedSettings, Profile,
};
use crate::remap;

#[derive(Clone, Debug)]
pub struct SettingsState {
    snapshot: AppSnapshot,
    fn_lock: bool,
    launch_at_startup: bool,
    config_path: PathBuf,
}

impl Default for SettingsState {
    fn default() -> Self {
        let config_path = config_file_path();
        let persisted = load_persisted(&config_path).unwrap_or_else(|_| default_persisted());
        let mut state = Self {
            snapshot: AppSnapshot {
                devices: Vec::new(),
                selected_device_id: None,
                profiles: persisted.profiles,
                remapper_active: false,
                status_message: "Starting…".into(),
                launch_at_startup: persisted.launch_at_startup,
                diagnostics: remap::diagnostics_snapshot(),
            },
            fn_lock: persisted.fn_lock,
            launch_at_startup: persisted.launch_at_startup,
            config_path,
        };
        let _ = autostart::sync(state.launch_at_startup);
        let _ = state.refresh_devices_inner();
        if let Some(profile_id) = state
            .snapshot
            .profiles
            .iter()
            .find(|profile| profile.id == persisted.active_profile_id)
            .map(|profile| profile.id.clone())
            .or_else(|| state.snapshot.profiles.first().map(|profile| profile.id.clone()))
        {
            for device in &mut state.snapshot.devices {
                device.active_profile_id = Some(profile_id.clone());
                device.fn_lock = state.fn_lock;
            }
            if state.snapshot.selected_device_id.is_none() {
                state.snapshot.selected_device_id =
                    state.snapshot.devices.first().map(|device| device.id.clone());
            }
        }
        let _ = state.apply_remapper();
        state
    }
}

impl SettingsState {
    pub fn snapshot(&self) -> AppSnapshot {
        let mut snap = self.snapshot.clone();
        snap.launch_at_startup = self.launch_at_startup;
        snap.diagnostics = remap::diagnostics_snapshot();
        snap.remapper_active = snap.diagnostics.hook_installed && snap.diagnostics.remapping_enabled;
        snap
    }

    pub fn refresh_devices(&mut self) -> AppSnapshot {
        let _ = self.refresh_devices_inner();
        let _ = self.apply_remapper();
        self.snapshot()
    }

    pub fn select_device(&mut self, device_id: &str) -> Result<AppSnapshot, SettingsError> {
        self.require_device(device_id)?;
        self.snapshot.selected_device_id = Some(device_id.to_owned());
        Ok(self.snapshot())
    }

    pub fn set_fn_lock(
        &mut self,
        device_id: &str,
        enabled: bool,
    ) -> Result<DeviceStatus, SettingsError> {
        let device = self.require_connected_device_mut(device_id)?;
        device.fn_lock = enabled;
        self.fn_lock = enabled;
        for other in &mut self.snapshot.devices {
            other.fn_lock = enabled;
        }
        self.persist()?;
        self.apply_remapper()?;
        Ok(self.require_device(device_id)?.clone())
    }

    pub fn set_launch_at_startup(&mut self, enabled: bool) -> Result<AppSnapshot, SettingsError> {
        autostart::sync(enabled).map_err(SettingsError::Message)?;
        self.launch_at_startup = enabled;
        self.snapshot.launch_at_startup = enabled;
        self.persist()?;
        Ok(self.snapshot())
    }

    pub fn set_active_profile(
        &mut self,
        device_id: &str,
        profile_id: &str,
    ) -> Result<DeviceStatus, SettingsError> {
        if !self.snapshot.profiles.iter().any(|profile| profile.id == profile_id) {
            return Err(SettingsError::NotFound("The selected profile does not exist."));
        }
        let device = self.require_connected_device_mut(device_id)?;
        device.active_profile_id = Some(profile_id.to_owned());
        for other in &mut self.snapshot.devices {
            other.active_profile_id = Some(profile_id.to_owned());
        }
        self.persist()?;
        self.apply_remapper()?;
        Ok(self.require_device(device_id)?.clone())
    }

    pub fn save_profile(&mut self, profile: Profile) -> Result<AppSnapshot, SettingsError> {
        validate_profile(&profile)?;
        let index = self
            .snapshot
            .profiles
            .iter()
            .position(|candidate| candidate.id == profile.id)
            .ok_or(SettingsError::NotFound("The profile does not exist."))?;
        self.snapshot.profiles[index] = profile;
        self.persist()?;
        self.apply_remapper()?;
        Ok(self.snapshot())
    }

    fn refresh_devices_inner(&mut self) -> Result<(), SettingsError> {
        let detected = detect::list_magic_keyboards()
            .map_err(SettingsError::Message)?;
        let previous_selected = self.snapshot.selected_device_id.clone();
        let active_profile = self
            .snapshot
            .devices
            .iter()
            .find_map(|device| device.active_profile_id.clone())
            .or_else(|| self.snapshot.profiles.first().map(|profile| profile.id.clone()));

        let devices = if detected.is_empty() {
            vec![DeviceStatus {
                id: "no-device".into(),
                name: "No Magic Keyboard detected".into(),
                model_id: "magic-keyboard-touch-id".into(),
                connection: ConnectionState::Disconnected,
                battery: None,
                fn_lock: self.fn_lock,
                active_profile_id: active_profile.clone(),
            }]
        } else {
            detected
                .into_iter()
                .map(|keyboard| to_device_status(keyboard, self.fn_lock, active_profile.clone()))
                .collect()
        };

        let selected = previous_selected
            .filter(|id| devices.iter().any(|device| &device.id == id && device.connection == ConnectionState::Connected))
            .or_else(|| {
                devices
                    .iter()
                    .find(|device| device.connection == ConnectionState::Connected)
                    .map(|device| device.id.clone())
            })
            .or_else(|| devices.first().map(|device| device.id.clone()));

        self.snapshot.devices = devices;
        self.snapshot.selected_device_id = selected;
        self.snapshot.status_message = if self
            .snapshot
            .devices
            .iter()
            .any(|device| device.connection == ConnectionState::Connected)
        {
            "Magic Keyboard detected. Remapping is active while this app is running.".into()
        } else {
            "Connect your Magic Keyboard over Bluetooth or USB, then click Refresh.".into()
        };
        Ok(())
    }

    fn apply_remapper(&mut self) -> Result<(), SettingsError> {
        remap::start().map_err(SettingsError::Message)?;
        let connected = self
            .snapshot
            .devices
            .iter()
            .any(|device| device.connection == ConnectionState::Connected);
        let profile_id = self
            .snapshot
            .devices
            .iter()
            .find(|device| device.connection == ConnectionState::Connected)
            .and_then(|device| device.active_profile_id.clone())
            .or_else(|| self.snapshot.profiles.first().map(|profile| profile.id.clone()));
        let mappings = profile_id
            .and_then(|id| {
                self.snapshot
                    .profiles
                    .iter()
                    .find(|profile| profile.id == id)
                    .map(|profile| profile.mappings.clone())
            })
            .unwrap_or_default();

        remap::update(connected, self.fn_lock, &mappings).map_err(SettingsError::Message)?;
        self.snapshot.diagnostics = remap::diagnostics_snapshot();
        self.snapshot.remapper_active = self.snapshot.diagnostics.hook_installed
            && self.snapshot.diagnostics.remapping_enabled;
        self.snapshot.status_message = if !self.snapshot.diagnostics.hook_installed {
            "Keyboard hook failed to install. Remapping is inactive.".into()
        } else if self.fn_lock {
            "Function lock ON: F-keys are standard. Hold Fn for brightness/media actions.".into()
        } else if connected {
            "Specials active: F11 lowers volume. Hold Fn+F11 for standard F11 (e.g. fullscreen).".into()
        } else {
            "Remapper active (no Magic Keyboard detected yet). Specials apply while this app is open.".into()
        };
        Ok(())
    }

    fn persist(&self) -> Result<(), SettingsError> {
        let active_profile_id = self
            .snapshot
            .devices
            .iter()
            .find_map(|device| device.active_profile_id.clone())
            .or_else(|| self.snapshot.profiles.first().map(|profile| profile.id.clone()))
            .unwrap_or_else(|| "standard".into());
        let payload = PersistedSettings {
            fn_lock: self.fn_lock,
            launch_at_startup: self.launch_at_startup,
            active_profile_id,
            profiles: self.snapshot.profiles.clone(),
        };
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                SettingsError::Message(format!("Could not create config folder: {error}"))
            })?;
        }
        let json = serde_json::to_string_pretty(&payload).map_err(|error| {
            SettingsError::Message(format!("Could not serialize settings: {error}"))
        })?;
        fs::write(&self.config_path, json).map_err(|error| {
            SettingsError::Message(format!("Could not save settings: {error}"))
        })?;
        Ok(())
    }

    fn require_device(&self, device_id: &str) -> Result<&DeviceStatus, SettingsError> {
        self.snapshot
            .devices
            .iter()
            .find(|device| device.id == device_id)
            .ok_or(SettingsError::NotFound("The selected device does not exist."))
    }

    fn require_connected_device_mut(
        &mut self,
        device_id: &str,
    ) -> Result<&mut DeviceStatus, SettingsError> {
        let device = self
            .snapshot
            .devices
            .iter_mut()
            .find(|device| device.id == device_id)
            .ok_or(SettingsError::NotFound("The selected device does not exist."))?;
        if device.connection != ConnectionState::Connected {
            return Err(SettingsError::Offline);
        }
        Ok(device)
    }
}

fn to_device_status(
    keyboard: DetectedKeyboard,
    fn_lock: bool,
    active_profile_id: Option<String>,
) -> DeviceStatus {
    DeviceStatus {
        id: keyboard.id,
        name: keyboard.name,
        model_id: keyboard.model_id.as_str().to_owned(),
        connection: ConnectionState::Connected,
        battery: None,
        fn_lock,
        active_profile_id,
    }
}

fn default_persisted() -> PersistedSettings {
    PersistedSettings {
        fn_lock: false,
        launch_at_startup: true,
        active_profile_id: "standard".into(),
        profiles: vec![Profile {
            id: "standard".into(),
            name: "Magic Keyboard".into(),
            mappings: default_function_row(),
        }],
    }
}

fn default_function_row() -> Vec<KeyMapping> {
    vec![
        KeyMapping {
            source_key: "F1".into(),
            action: KeyAction::Key {
                key: "BrightnessDown".into(),
            },
        },
        KeyMapping {
            source_key: "F2".into(),
            action: KeyAction::Key {
                key: "BrightnessUp".into(),
            },
        },
        KeyMapping {
            source_key: "F3".into(),
            action: KeyAction::Shortcut {
                keys: vec!["Meta".into(), "Tab".into()],
            },
        },
        KeyMapping {
            source_key: "F4".into(),
            action: KeyAction::Shortcut {
                keys: vec!["Meta".into(), "S".into()],
            },
        },
        KeyMapping {
            source_key: "F5".into(),
            action: KeyAction::Shortcut {
                keys: vec!["Meta".into(), "H".into()],
            },
        },
        KeyMapping {
            source_key: "F6".into(),
            action: KeyAction::Key {
                key: "Sleep".into(),
            },
        },
        KeyMapping {
            source_key: "F7".into(),
            action: KeyAction::Media {
                command: MediaCommand::Previous,
            },
        },
        KeyMapping {
            source_key: "F8".into(),
            action: KeyAction::Media {
                command: MediaCommand::PlayPause,
            },
        },
        KeyMapping {
            source_key: "F9".into(),
            action: KeyAction::Media {
                command: MediaCommand::Next,
            },
        },
        KeyMapping {
            source_key: "F10".into(),
            action: KeyAction::Media {
                command: MediaCommand::Mute,
            },
        },
        KeyMapping {
            source_key: "F11".into(),
            action: KeyAction::Media {
                command: MediaCommand::VolumeDown,
            },
        },
        KeyMapping {
            source_key: "F12".into(),
            action: KeyAction::Media {
                command: MediaCommand::VolumeUp,
            },
        },
        // A1644 Eject is invisible to user-mode apps on Windows. Caps Lock is
        // the interim Delete key until a HID filter rewrites Eject in-kernel.
        KeyMapping {
            source_key: "CapsLock".into(),
            action: KeyAction::Key {
                key: "Delete".into(),
            },
        },
    ]
}

fn config_file_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("MagicKeyboard").join("settings.json")
}

fn load_persisted(path: &PathBuf) -> Result<PersistedSettings, ()> {
    let bytes = fs::read(path).map_err(|_| ())?;
    serde_json::from_slice(&bytes).map_err(|_| ())
}

fn validate_profile(profile: &Profile) -> Result<(), SettingsError> {
    if profile.name.trim().is_empty() || profile.name.encode_utf16().count() > 40 {
        return Err(SettingsError::InvalidProfile(
            "Profile name must contain between 1 and 40 characters.",
        ));
    }
    let mut seen = HashSet::new();
    for mapping in &profile.mappings {
        let key = mapping.source_key.trim().to_ascii_lowercase();
        if key.is_empty() || !seen.insert(key) {
            return Err(SettingsError::InvalidProfile(
                "Each source key can be mapped only once per profile.",
            ));
        }
        validate_action(&mapping.action)?;
    }
    Ok(())
}

fn validate_action(action: &KeyAction) -> Result<(), SettingsError> {
    match action {
        KeyAction::Disabled | KeyAction::Media { .. } | KeyAction::Key { .. } => Ok(()),
        KeyAction::Shortcut { keys } => {
            if !(1..=4).contains(&keys.len()) {
                return Err(SettingsError::InvalidProfile(
                    "A shortcut must contain between one and four keys.",
                ));
            }
            Ok(())
        }
        KeyAction::Launch { target } => {
            let length = target.trim().encode_utf16().count();
            if (1..=260).contains(&length) {
                Ok(())
            } else {
                Err(SettingsError::InvalidProfile(
                    "Enter an application target of 260 characters or fewer.",
                ))
            }
        }
    }
}

#[derive(Debug)]
pub enum SettingsError {
    NotFound(&'static str),
    Offline,
    InvalidProfile(&'static str),
    Message(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(message) | Self::InvalidProfile(message) => formatter.write_str(message),
            Self::Offline => formatter.write_str("The selected device is not connected."),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}
