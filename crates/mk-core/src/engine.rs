use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::{
    validate_config, Action, ButtonState, ConfigDocument, ControlCode, DeviceId, DeviceIdentity,
    DeviceSelector, InputEvent, InputEventKind, Profile, ProfileId, RuleId, ValidationErrors,
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    InvalidConfig(#[from] ValidationErrors),
    #[error("profile '{0}' does not exist")]
    UnknownProfile(ProfileId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineOutput {
    pub profile_id: Option<ProfileId>,
    pub matched_rules: Vec<RuleId>,
    pub actions: Vec<Action>,
}

/// Deterministically select the best profile for a device.
///
/// Device-specific profiles win over the configured default and generic
/// profiles. Ties use descending selection priority and then ascending profile
/// id, making selection independent of JSON array order.
pub fn select_profile<'a>(
    config: &'a ConfigDocument,
    device_id: &DeviceId,
    device: Option<&DeviceIdentity>,
) -> Option<&'a Profile> {
    let mut specific: Vec<_> = config
        .profiles
        .iter()
        .filter(|profile| {
            !profile.selectors.is_empty()
                && profile
                    .selectors
                    .iter()
                    .any(|selector| selector_matches(selector, device_id, device))
        })
        .collect();
    rank_profiles(&mut specific);

    specific.into_iter().next().or_else(|| {
        config
            .default_profile
            .as_ref()
            .and_then(|id| config.profiles.iter().find(|profile| &profile.id == id))
            .or_else(|| {
                let mut generic: Vec<_> = config
                    .profiles
                    .iter()
                    .filter(|profile| profile.selectors.is_empty())
                    .collect();
                rank_profiles(&mut generic);
                generic.into_iter().next()
            })
    })
}

fn rank_profiles(profiles: &mut Vec<&Profile>) {
    profiles.sort_by(|left, right| {
        right
            .selection_priority
            .cmp(&left.selection_priority)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn selector_matches(
    selector: &DeviceSelector,
    device_id: &DeviceId,
    device: Option<&DeviceIdentity>,
) -> bool {
    match selector {
        DeviceSelector::Id { id } => id == device_id,
        _ => device.is_some_and(|identity| selector.matches(identity)),
    }
}

/// Stateful, deterministic translator from normalized events to actions.
pub struct MappingEngine {
    config: ConfigDocument,
    active_profile: Option<ProfileId>,
    held: BTreeMap<DeviceId, BTreeSet<ControlCode>>,
}

impl MappingEngine {
    pub fn new(config: ConfigDocument) -> Result<Self, EngineError> {
        validate_config(&config)?;
        Ok(Self {
            config,
            active_profile: None,
            held: BTreeMap::new(),
        })
    }

    pub fn config(&self) -> &ConfigDocument {
        &self.config
    }

    pub fn active_profile(&self) -> Option<&ProfileId> {
        self.active_profile.as_ref()
    }

    pub fn set_active_profile(&mut self, profile_id: Option<ProfileId>) -> Result<(), EngineError> {
        if let Some(id) = &profile_id {
            if !self.config.profiles.iter().any(|profile| &profile.id == id) {
                return Err(EngineError::UnknownProfile(id.clone()));
            }
        }
        self.active_profile = profile_id;
        Ok(())
    }

    pub fn held_controls(&self, device_id: &DeviceId) -> Option<&BTreeSet<ControlCode>> {
        self.held.get(device_id)
    }

    pub fn process(
        &mut self,
        event: &InputEvent,
        device: Option<&DeviceIdentity>,
    ) -> EngineOutput {
        let held_before = self.held.get(&event.device_id).cloned().unwrap_or_default();
        let profile = self
            .active_profile
            .as_ref()
            .and_then(|id| self.config.profiles.iter().find(|profile| &profile.id == id))
            .or_else(|| select_profile(&self.config, &event.device_id, device));
        let selected_profile_id = profile.map(|profile| profile.id.clone());

        let mut matched_rules = Vec::new();
        let mut actions = Vec::new();

        if let (Some(profile), InputEventKind::Button {
            control,
            state,
            repeat,
        }) = (profile, &event.event)
        {
            let mut rules: Vec<_> = profile
                .mappings
                .iter()
                .filter(|rule| {
                    rule.enabled
                        && rule.trigger.control == *control
                        && rule.trigger.state == *state
                        && rule
                            .trigger
                            .device_id
                            .as_ref()
                            .map_or(true, |id| id == &event.device_id)
                        && (!*repeat || rule.trigger.allow_repeat)
                        && rule.trigger.required_held.is_subset(&held_before)
                })
                .collect();

            rules.sort_by(|left, right| {
                right
                    .priority
                    .cmp(&left.priority)
                    .then_with(|| left.id.cmp(&right.id))
            });

            for rule in rules {
                matched_rules.push(rule.id.clone());
                actions.extend(rule.actions.iter().cloned());
                if rule.consume {
                    break;
                }
            }
        }

        self.update_held(event);

        for action in &actions {
            if let Action::SwitchProfile { profile_id } = action {
                // Validation guarantees that action targets exist.
                self.active_profile = Some(profile_id.clone());
            }
        }

        EngineOutput {
            profile_id: selected_profile_id,
            matched_rules,
            actions,
        }
    }

    fn update_held(&mut self, event: &InputEvent) {
        let InputEventKind::Button {
            control, state, ..
        } = &event.event
        else {
            return;
        };

        let held = self.held.entry(event.device_id.clone()).or_default();
        match state {
            ButtonState::Pressed => {
                held.insert(*control);
            }
            ButtonState::Released => {
                held.remove(control);
                if held.is_empty() {
                    self.held.remove(&event.device_id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{MappingRule, Trigger, CURRENT_SCHEMA_VERSION};

    fn profile(id: &str, mappings: Vec<MappingRule>) -> Profile {
        Profile {
            id: ProfileId::new(id),
            name: None,
            selectors: Vec::new(),
            selection_priority: 0,
            mappings,
        }
    }

    fn rule(id: &str, priority: i32, required_held: BTreeSet<ControlCode>) -> MappingRule {
        MappingRule {
            id: RuleId::new(id),
            enabled: true,
            priority,
            trigger: Trigger {
                control: ControlCode::keyboard(0x04),
                state: ButtonState::Pressed,
                device_id: None,
                required_held,
                allow_repeat: false,
            },
            actions: vec![Action::Log {
                message: id.to_owned(),
            }],
            consume: false,
        }
    }

    #[test]
    fn rules_are_ranked_by_priority_then_id() {
        let config = ConfigDocument {
            schema_version: CURRENT_SCHEMA_VERSION,
            profiles: vec![profile(
                "default",
                vec![
                    rule("z-low", 0, BTreeSet::new()),
                    rule("b-high", 10, BTreeSet::new()),
                    rule("a-high", 10, BTreeSet::new()),
                ],
            )],
            default_profile: Some(ProfileId::new("default")),
            ..ConfigDocument::default()
        };
        let mut engine = MappingEngine::new(config).unwrap();

        let output = engine.process(
            &InputEvent::button(
                DeviceId::new("keyboard"),
                1,
                10,
                ControlCode::keyboard(0x04),
                ButtonState::Pressed,
            ),
            None,
        );

        assert_eq!(
            output.matched_rules,
            vec![
                RuleId::new("a-high"),
                RuleId::new("b-high"),
                RuleId::new("z-low")
            ]
        );
    }

    #[test]
    fn required_controls_use_state_before_current_event() {
        let modifier = ControlCode::keyboard(0xe1);
        let config = ConfigDocument {
            schema_version: CURRENT_SCHEMA_VERSION,
            profiles: vec![profile(
                "default",
                vec![rule("shift-a", 0, BTreeSet::from([modifier]))],
            )],
            default_profile: Some(ProfileId::new("default")),
            ..ConfigDocument::default()
        };
        let mut engine = MappingEngine::new(config).unwrap();
        let device = DeviceId::new("keyboard");

        engine.process(
            &InputEvent::button(
                device.clone(),
                1,
                10,
                modifier,
                ButtonState::Pressed,
            ),
            None,
        );
        let output = engine.process(
            &InputEvent::button(
                device,
                2,
                20,
                ControlCode::keyboard(0x04),
                ButtonState::Pressed,
            ),
            None,
        );

        assert_eq!(output.matched_rules, vec![RuleId::new("shift-a")]);
    }

    #[test]
    fn profile_ties_use_lexical_id_not_document_order() {
        let selector = DeviceSelector::Id {
            id: DeviceId::new("keyboard"),
        };
        let mut second = profile("z-profile", Vec::new());
        second.selectors.push(selector.clone());
        let mut first = profile("a-profile", Vec::new());
        first.selectors.push(selector);
        let config = ConfigDocument {
            schema_version: CURRENT_SCHEMA_VERSION,
            profiles: vec![second, first],
            ..ConfigDocument::default()
        };

        assert_eq!(
            select_profile(&config, &DeviceId::new("keyboard"), None).map(|value| &value.id),
            Some(&ProfileId::new("a-profile"))
        );
    }
}
