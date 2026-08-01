use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

use crate::{validate_config, ConfigDocument, ConfigParseError, ValidationErrors};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("cannot use configuration path '{}'", .0.display())]
    InvalidPath(PathBuf),
    #[error("failed to {operation} '{}': {source}", path.display())]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to serialize configuration: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error(transparent)]
    Parse(#[from] ConfigParseError),
    #[error(transparent)]
    Invalid(#[from] ValidationErrors),
}

pub fn load_config(path: impl AsRef<Path>) -> Result<ConfigDocument, PersistenceError> {
    let path = path.as_ref();
    let json = fs::read_to_string(path).map_err(|source| PersistenceError::Io {
        operation: "read",
        path: path.to_path_buf(),
        source,
    })?;
    ConfigDocument::from_json(&json).map_err(PersistenceError::from)
}

/// Validate and persist a configuration through a same-directory temporary.
///
/// `rename` provides atomic replacement on platforms that support replacing an
/// existing file. Windows' standard-library rename cannot replace an existing
/// file, so that case uses a recoverable backup/rename sequence instead.
pub fn save_config_atomic(
    path: impl AsRef<Path>,
    config: &ConfigDocument,
) -> Result<(), PersistenceError> {
    let path = path.as_ref();
    validate_config(config)?;
    let mut json = config
        .to_json_pretty()
        .map_err(PersistenceError::Serialize)?;
    json.push('\n');

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|source| PersistenceError::Io {
                operation: "create parent directory for",
                path: path.to_path_buf(),
                source,
            })?;
        }
    }

    let temporary = unique_sibling(path, "tmp")?;
    let write_result = write_temporary(&temporary, json.as_bytes());
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    if let Err(error) = replace_file(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

fn write_temporary(path: &Path, contents: &[u8]) -> Result<(), PersistenceError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| PersistenceError::Io {
            operation: "create temporary file",
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|source| PersistenceError::Io {
            operation: "write temporary file",
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, target: &Path) -> Result<(), PersistenceError> {
    fs::rename(temporary, target).map_err(|source| PersistenceError::Io {
        operation: "replace",
        path: target.to_path_buf(),
        source,
    })
}

#[cfg(windows)]
fn replace_file(temporary: &Path, target: &Path) -> Result<(), PersistenceError> {
    if !target.exists() {
        return fs::rename(temporary, target).map_err(|source| PersistenceError::Io {
            operation: "install",
            path: target.to_path_buf(),
            source,
        });
    }

    let backup = unique_sibling(target, "backup")?;
    fs::rename(target, &backup).map_err(|source| PersistenceError::Io {
        operation: "back up",
        path: target.to_path_buf(),
        source,
    })?;

    if let Err(source) = fs::rename(temporary, target) {
        let _ = fs::rename(&backup, target);
        return Err(PersistenceError::Io {
            operation: "install",
            path: target.to_path_buf(),
            source,
        });
    }

    fs::remove_file(&backup).map_err(|source| PersistenceError::Io {
        operation: "remove backup for",
        path: target.to_path_buf(),
        source,
    })
}

fn unique_sibling(path: &Path, suffix: &str) -> Result<PathBuf, PersistenceError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PersistenceError::InvalidPath(path.to_path_buf()))?;
    let number = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(path.with_file_name(format!(
        ".{file_name}.{}.{}.{}",
        std::process::id(),
        number,
        suffix
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Profile, ProfileId, CURRENT_SCHEMA_VERSION};

    fn temporary_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "mk-core-test-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn saves_loads_and_replaces_config() {
        let directory = temporary_directory();
        let path = directory.join("config.json");
        let mut config = ConfigDocument {
            schema_version: CURRENT_SCHEMA_VERSION,
            profiles: vec![Profile {
                id: ProfileId::new("first"),
                name: None,
                selectors: Vec::new(),
                selection_priority: 0,
                mappings: Vec::new(),
            }],
            default_profile: Some(ProfileId::new("first")),
            ..ConfigDocument::default()
        };

        save_config_atomic(&path, &config).unwrap();
        assert_eq!(load_config(&path).unwrap(), config);

        config.profiles[0].name = Some("Updated".to_owned());
        save_config_atomic(&path, &config).unwrap();
        assert_eq!(load_config(&path).unwrap(), config);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_unknown_schema_before_deserializing_model() {
        let error = ConfigDocument::from_json(r#"{"schema_version":99}"#).unwrap_err();
        assert!(matches!(
            error,
            ConfigParseError::UnsupportedSchemaVersion(99)
        ));
    }
}
