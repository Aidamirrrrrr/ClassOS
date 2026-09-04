//! Минимальная конфигурация T0: TOML с `log_level` и постоянный device id.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AgentError, Result};

/// Корневой каталог runtime-состояния ClassOS в Windows.
pub const PROGRAM_DATA_DIR: &str = r"C:\ProgramData\ClassOS";

/// Путь к конфигурации T0.
pub fn config_file_path() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR).join("config.toml")
}

/// Путь к постоянному device id.
pub fn device_id_path() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR)
        .join("state")
        .join("device-id")
}

/// Путь к публичному сертификату device identity T1.
pub fn device_certificate_path() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR)
        .join("state")
        .join("device-certificate.der")
}

/// Путь к закрытому ключу device identity, защищённому Windows DPAPI.
pub fn protected_device_key_path() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR)
        .join("state")
        .join("device-key.dpapi")
}

/// Путь к одноразовому коду, введённому на Student PC.
pub fn pending_enrollment_code_path() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR)
        .join("state")
        .join("pending-enrollment-code")
}

/// Путь к подписанному credential устройства.
pub fn device_credential_path() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR)
        .join("state")
        .join("device-credential.bin")
}

/// Путь к публичному ключу Teacher issuer.
pub fn teacher_issuer_key_path() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR)
        .join("state")
        .join("teacher-issuer-public-key.bin")
}

/// Путь к сохранённому состоянию политики T6.
pub fn policy_state_path() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR)
        .join("state")
        .join("policy.toml")
}

/// Каталог снимков состояния устройства до применения политики.
///
/// Снимки обязаны переживать перезагрузку: без них откат после сбоя Service
/// оставил бы устройство заблокированным (ADR-0014).
pub fn policy_snapshot_dir() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR)
        .join("state")
        .join("policy-snapshots")
}

/// Рабочий каталог для XML-файлов политики, передаваемых в AppLocker.
pub fn policy_workspace_dir() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR).join("state").join("policy")
}

/// Каталог журналов.
pub fn log_dir() -> PathBuf {
    PathBuf::from(PROGRAM_DATA_DIR).join("logs")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Желаемый software-профиль кабинета. В T7 задаётся локально; реальная
    /// привязка Room → профиль появляется вместе с Cloud v0 (T8).
    #[serde(default = "default_software_profile")]
    pub software_profile_id: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_software_profile() -> String {
    "python-classroom".to_string()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            software_profile_id: default_software_profile(),
        }
    }
}

impl AgentConfig {
    /// Загружает конфигурацию; при отсутствии файла использует значения по
    /// умолчанию. Ошибочный файл возвращает явную ошибку.
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

    /// Загружает конфигурацию из стандартного каталога T0.
    pub fn load() -> Result<Self> {
        Self::load_from(&config_file_path())
    }
}

/// Загружает постоянный device id или создаёт и сохраняет новый UUID v4.
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

/// Создаёт новый непостоянный service instance id.
pub fn new_service_instance_id() -> Uuid {
    Uuid::new_v4()
}

/// Создаёт новый непостоянный session instance id.
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
