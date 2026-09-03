//! Minimal T0 configuration (spec §119-122): a tiny TOML config file plus a
//! persisted device identifier. Deliberately not a general config
//! framework — T0 only needs `log_level` and a device id.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AgentError, Result};

/// Root directory for all ClassOS runtime state on a Windows machine
/// (spec §93, §119, §120).
pub const PROGRAM_DATA_DIR: &str = r"C:\ProgramData\ClassOS";

/// Path to the T0 config file (spec §119).
pub fn config_file_path() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR).join("config.toml")
}

/// Path to the persisted device id (spec §120).
pub fn device_id_path() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR)
        .join("state")
        .join("device-id")
}

/// Log directory (spec §80).
pub fn log_dir() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR).join("logs")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
        }
    }
}

impl AgentConfig {
    /// Loads config from `path`, falling back to defaults if the file does
    /// not exist. A malformed file is a hard error rather than silently
    /// ignored, so misconfiguration is visible.
    pub fn load_from(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents).map_err(|err| AgentError::Config {
                reason: format!("failed to parse {}: {err}", path.display()),
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(AgentError::Config {
                reason: format!("failed to read {}: {err}", path.display()),
            }),
        }
    }

    /// Loads config from the default T0 location.
    pub fn load() -> Result<Self> {
        Self::load_from(&config_file_path())
    }
}

/// Loads the persistent device id from `path`, creating a new random UUID
/// v4 and persisting it if none exists yet (spec §120).
pub fn load_or_create_device_id(path: &Path) -> Result<Uuid> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Uuid::parse_str(contents.trim()).map_err(|err| AgentError::Config {
            reason: format!("invalid device id in {}: {err}", path.display()),
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let id = Uuid::new_v4();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| AgentError::Config {
                    reason: format!("failed to create {}: {err}", parent.display()),
                })?;
            }
            std::fs::write(path, id.to_string()).map_err(|err| AgentError::Config {
                reason: format!("failed to persist device id to {}: {err}", path.display()),
            })?;
            Ok(id)
        }
        Err(err) => Err(AgentError::Config {
            reason: format!("failed to read {}: {err}", path.display()),
        }),
    }
}

/// Generates a fresh, non-persistent service instance id (spec §121).
pub fn new_service_instance_id() -> Uuid {
    Uuid::new_v4()
}

/// Generates a fresh, non-persistent session instance id for a newly
/// launched Session Host (spec §122).
pub fn new_session_instance_id() -> Uuid {
    Uuid::new_v4()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_info_log_level() {
        assert_eq!(AgentConfig::default().log_level, "info");
    }

    #[test]
    fn load_from_missing_file_uses_default() {
        let dir = tempdir();
        let path = dir.join("nonexistent-config.toml");
        let config = AgentConfig::load_from(&path).unwrap();
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn load_from_valid_file_parses_log_level() {
        let dir = tempdir();
        let path = dir.join("config.toml");
        std::fs::write(&path, "log_level = \"debug\"\n").unwrap();
        let config = AgentConfig::load_from(&path).unwrap();
        assert_eq!(config.log_level, "debug");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_from_malformed_file_errors() {
        let dir = tempdir();
        let path = dir.join("bad-config.toml");
        std::fs::write(&path, "not valid toml {{{").unwrap();
        let result = AgentConfig::load_from(&path);
        assert!(result.is_err());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn device_id_is_created_once_and_reused() {
        let dir = tempdir();
        let path = dir.join("device-id");
        let first = load_or_create_device_id(&path).unwrap();
        let second = load_or_create_device_id(&path).unwrap();
        assert_eq!(first, second);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn service_and_session_instance_ids_are_unique() {
        assert_ne!(new_service_instance_id(), new_service_instance_id());
        assert_ne!(new_session_instance_id(), new_session_instance_id());
    }

    fn tempdir() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("classos-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
