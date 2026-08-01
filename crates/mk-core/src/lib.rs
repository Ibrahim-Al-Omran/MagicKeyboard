//! Platform-neutral configuration, device, event, and mapping primitives.
//!
//! This crate intentionally does not open HID devices or call operating-system
//! APIs. Platform adapters can translate native reports into [`InputEvent`]s
//! and execute the [`Action`]s returned by [`MappingEngine`].

#![forbid(unsafe_code)]

mod config;
mod device;
mod engine;
mod event;
mod mapping;
mod persistence;
mod validation;

pub use config::{
    ConfigDocument, ConfigParseError, Profile, ProfileId, ServiceConfig, CURRENT_SCHEMA_VERSION,
};
pub use device::{DeviceId, DeviceIdentity, DeviceSelector, Transport};
pub use engine::{select_profile, EngineError, EngineOutput, MappingEngine};
pub use event::{ButtonState, ControlCode, InputEvent, InputEventKind};
pub use mapping::{Action, MappingRule, RuleId, Trigger};
pub use persistence::{load_config, save_config_atomic, PersistenceError};
pub use validation::{validate_config, ValidationErrors, ValidationIssue};
