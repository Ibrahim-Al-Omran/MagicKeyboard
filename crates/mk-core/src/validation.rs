use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Action, ConfigDocument, CURRENT_SCHEMA_VERSION};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ValidationIssue {
    pub path: String,
    pub code: String,
    pub message: String,
}

impl ValidationIssue {
    fn new(path: impl Into<String>, code: &str, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationErrors {
    issues: Vec<ValidationIssue>,
}

impl ValidationErrors {
    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }

    pub fn into_issues(self) -> Vec<ValidationIssue> {
        self.issues
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "configuration has {} validation issue(s)",
            self.issues.len()
        )
    }
}

impl Error for ValidationErrors {}

pub fn validate_config(config: &ConfigDocument) -> Result<(), ValidationErrors> {
    let mut issues = Vec::new();

    if config.schema_version != CURRENT_SCHEMA_VERSION {
        issues.push(ValidationIssue::new(
            "schema_version",
            "unsupported_schema",
            format!(
                "expected schema version {CURRENT_SCHEMA_VERSION}, got {}",
                config.schema_version
            ),
        ));
    }

    let mut device_ids = BTreeSet::new();
    for (index, device) in config.devices.iter().enumerate() {
        let path = format!("devices[{index}].id");
        check_identifier(&device.id.0, &path, &mut issues);
        if !device_ids.insert(device.id.0.as_str()) {
            issues.push(ValidationIssue::new(
                path,
                "duplicate_id",
                format!("device id '{}' is duplicated", device.id),
            ));
        }
    }

    let mut profile_ids = BTreeSet::new();
    for (index, profile) in config.profiles.iter().enumerate() {
        let path = format!("profiles[{index}].id");
        check_identifier(&profile.id.0, &path, &mut issues);
        if !profile_ids.insert(profile.id.0.as_str()) {
            issues.push(ValidationIssue::new(
                path,
                "duplicate_id",
                format!("profile id '{}' is duplicated", profile.id),
            ));
        }
    }

    if let Some(default_profile) = &config.default_profile {
        if !profile_ids.contains(default_profile.0.as_str()) {
            issues.push(ValidationIssue::new(
                "default_profile",
                "unknown_profile",
                format!("profile '{}' does not exist", default_profile),
            ));
        }
    }

    let mut rule_ids = BTreeSet::new();
    for (profile_index, profile) in config.profiles.iter().enumerate() {
        for (rule_index, rule) in profile.mappings.iter().enumerate() {
            let base = format!("profiles[{profile_index}].mappings[{rule_index}]");
            check_identifier(&rule.id.0, &format!("{base}.id"), &mut issues);
            if !rule_ids.insert(rule.id.0.as_str()) {
                issues.push(ValidationIssue::new(
                    format!("{base}.id"),
                    "duplicate_id",
                    format!("mapping rule id '{}' is duplicated", rule.id),
                ));
            }

            if rule.actions.is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{base}.actions"),
                    "empty_actions",
                    "a mapping rule must contain at least one action",
                ));
            }

            for (action_index, action) in rule.actions.iter().enumerate() {
                let action_path = format!("{base}.actions[{action_index}]");
                match action {
                    Action::RunCommand { program, .. } if program.trim().is_empty() => {
                        issues.push(ValidationIssue::new(
                            format!("{action_path}.program"),
                            "empty_program",
                            "command program cannot be empty",
                        ));
                    }
                    Action::SwitchProfile { profile_id }
                        if !profile_ids.contains(profile_id.0.as_str()) =>
                    {
                        issues.push(ValidationIssue::new(
                            format!("{action_path}.profile_id"),
                            "unknown_profile",
                            format!("profile '{}' does not exist", profile_id),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors { issues })
    }
}

fn check_identifier(value: &str, path: &str, issues: &mut Vec<ValidationIssue>) {
    if value.trim().is_empty() {
        issues.push(ValidationIssue::new(
            path,
            "empty_id",
            "identifier cannot be empty",
        ));
    } else if value != value.trim() {
        issues.push(ValidationIssue::new(
            path,
            "invalid_id",
            "identifier cannot have leading or trailing whitespace",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Profile, ProfileId};

    #[test]
    fn reports_duplicate_and_unknown_profile_ids_together() {
        let fallback = Profile {
            id: ProfileId::new("same"),
            name: None,
            selectors: Vec::new(),
            selection_priority: 0,
            mappings: Vec::new(),
        };
        let config = ConfigDocument {
            profiles: vec![fallback.clone(), fallback],
            default_profile: Some(ProfileId::new("missing")),
            ..ConfigDocument::default()
        };

        let errors = validate_config(&config).unwrap_err();
        let codes: BTreeSet<_> = errors
            .issues()
            .iter()
            .map(|issue| issue.code.as_str())
            .collect();

        assert!(codes.contains("duplicate_id"));
        assert!(codes.contains("unknown_profile"));
    }
}
