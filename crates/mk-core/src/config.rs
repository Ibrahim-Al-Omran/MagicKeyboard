use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{validate_config, DeviceIdentity, DeviceSelector, MappingRule, ValidationErrors};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(
    Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct ProfileId(pub String);

impl ProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ServiceConfig {
    #[serde(default = "default_true")]
    pub driver_enabled: bool,
    #[serde(default)]
    pub log_events: bool,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            driver_enabled: true,
            log_events: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Profile {
    pub id: ProfileId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// An empty selector list marks a generic fallback profile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selectors: Vec<DeviceSelector>,
    #[serde(default)]
    pub selection_priority: i32,
    #[serde(default)]
    pub mappings: Vec<MappingRule>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConfigDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub service: ServiceConfig,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<DeviceIdentity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<Profile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<ProfileId>,
}

impl Default for ConfigDocument {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            service: ServiceConfig::default(),
            devices: Vec::new(),
            profiles: Vec::new(),
            default_profile: None,
        }
    }
}

impl ConfigDocument {
    /// Parse a configuration only if its schema is supported and valid.
    pub fn from_json(json: &str) -> Result<Self, ConfigParseError> {
        let value: serde_json::Value = serde_json::from_str(json)?;
        let schema_version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or(ConfigParseError::MissingSchemaVersion)?;

        if schema_version != u64::from(CURRENT_SCHEMA_VERSION) {
            return Err(ConfigParseError::UnsupportedSchemaVersion(schema_version));
        }

        let config: Self = serde_json::from_value(value)?;
        validate_config(&config).map_err(ConfigParseError::Invalid)?;
        Ok(config)
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[derive(Debug, Error)]
pub enum ConfigParseError {
    #[error("configuration is missing an integer schema_version")]
    MissingSchemaVersion,
    #[error("unsupported configuration schema version {0}")]
    UnsupportedSchemaVersion(u64),
    #[error("configuration JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Invalid(ValidationErrors),
}

const fn default_true() -> bool {
    true
}
