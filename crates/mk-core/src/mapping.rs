use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{ButtonState, ControlCode, DeviceId, ProfileId};

#[derive(
    Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct RuleId(pub String);

impl RuleId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Conditions for a normalized button event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Trigger {
    pub control: ControlCode,
    pub state: ButtonState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<DeviceId>,
    /// Every listed control must already be held when the event arrives.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_held: BTreeSet<ControlCode>,
    #[serde(default)]
    pub allow_repeat: bool,
}

impl Trigger {
    pub fn button(control: ControlCode, state: ButtonState) -> Self {
        Self {
            control,
            state,
            device_id: None,
            required_held: BTreeSet::new(),
            allow_repeat: false,
        }
    }
}

/// Platform-neutral intent emitted by the mapping engine.
///
/// The service decides how each intent is executed. In particular, merely
/// deserializing a `RunCommand` never executes it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    EmitButton {
        control: ControlCode,
        state: ButtonState,
    },
    RunCommand {
        program: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        environment: BTreeMap<String, String>,
    },
    SwitchProfile {
        profile_id: ProfileId,
    },
    SetDriverEnabled {
        enabled: bool,
    },
    Log {
        message: String,
    },
    NoOp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MappingRule {
    pub id: RuleId,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    pub trigger: Trigger,
    #[serde(default)]
    pub actions: Vec<Action>,
    /// Stop evaluating lower-ranked matching rules.
    #[serde(default = "default_true")]
    pub consume: bool,
}

const fn default_true() -> bool {
    true
}
