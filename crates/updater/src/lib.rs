//! Проверка и применение обновлений агента.
//!
//! Обновление проходит конвейер `download → verify hash → verify signature →
//! stage → install → health check` и **обязано** откатываться при провале
//! health check (spec T8 §8.2, §8.4; инвариант IV `CLAUDE.md`).
//!
//! Крейт переносим: замена файлов и перезапуск службы скрыты за трейтами,
//! поэтому весь конвейер, включая откат, проверяется тестами на любом хосте.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(windows)]
pub mod windows_store;

/// Канал обновлений устройства.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Beta,
    Canary,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Canary => "canary",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            "canary" => Some(Self::Canary),
            _ => None,
        }
    }
}

/// Манифест обновления, опубликованный Cloud.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub url: String,
    /// SHA-256 файла обновления в шестнадцатеричном виде.
    pub sha256: String,
    /// Подпись издателя в hex: 64 байта.
    #[serde(with = "signature_hex")]
    pub signature: [u8; 64],
    /// Версия, ниже которой обновление ставить нельзя: обычно из-за
    /// несовместимого формата состояния на диске.
    pub minimum_supported_version: String,
    pub release_channel: String,
}

mod signature_hex {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error> {
        let hex: String = value.iter().map(|byte| format!("{byte:02x}")).collect();
        serializer.serialize_str(&hex)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 64], D::Error> {
        let hex = String::deserialize(deserializer)?;
        if hex.len() != 128 {
            return Err(serde::de::Error::custom("подпись должна быть 64 байта"));
        }
        let mut bytes = [0_u8; 64];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
                .map_err(serde::de::Error::custom)?;
        }
        Ok(bytes)
    }
}

/// Разбирает манифест, опубликованный Cloud.
pub fn parse_manifest(text: &str) -> Result<UpdateManifest, UpdateError> {
    serde_json::from_str(text).map_err(|error| UpdateError::InvalidVersion(error.to_string()))
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UpdateError {
    #[error("сборка выполнена без ключа издателя обновлений")]
    MissingPublisherKey,
    #[error("некорректный публичный ключ издателя")]
    InvalidPublisherKey,
    #[error("подпись манифеста не прошла проверку")]
    InvalidSignature,
    #[error("хеш файла обновления не совпадает с манифестом")]
    HashMismatch,
    #[error("некорректная версия в манифесте: {0}")]
    InvalidVersion(String),
    #[error("манифест предназначен другому каналу обновлений")]
    ChannelMismatch,
    #[error("текущая версия слишком старая для этого обновления")]
    UnsupportedCurrentVersion,
    #[error("установка обновления: {0}")]
    Install(String),
    #[error("health check после обновления не пройден: {0}")]
    HealthCheckFailed(String),
    #[error("откат обновления: {0}")]
    Rollback(String),
}

/// Канонические байты манифеста для подписи.
///
/// Поля разделяются длиной, а не разделителем: иначе версию и URL можно было
/// бы переставить местами, сохранив подпись.
fn manifest_payload(manifest: &UpdateManifest) -> Vec<u8> {
    let mut bytes = Vec::new();
    for field in [
        manifest.version.as_str(),
        manifest.url.as_str(),
        manifest.sha256.as_str(),
        manifest.minimum_supported_version.as_str(),
        manifest.release_channel.as_str(),
    ] {
        bytes.extend_from_slice(&u32::try_from(field.len()).unwrap_or(u32::MAX).to_be_bytes());
        bytes.extend_from_slice(field.as_bytes());
    }
    bytes
}

/// Подписывает манифест. Используется издателем и тестами.
pub fn sign_manifest(
    signing_key: &ed25519_dalek::SigningKey,
    manifest: &UpdateManifest,
) -> [u8; 64] {
    use ed25519_dalek::Signer;
    signing_key.sign(&manifest_payload(manifest)).to_bytes()
}

/// Решение по манифесту.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDecision {
    /// Обновление применимо.
    Apply,
    /// Версия не новее установленной.
    AlreadyCurrent,
}

/// Публичный ключ издателя, вшитый в бинарник при сборке релиза.
///
/// Ключ, лежащий рядом на диске, обесценил бы проверку подписи (spec T8
/// §12.3), поэтому он приходит из переменной окружения сборки
/// `CLASSOS_PUBLISHER_KEY_HEX`. Сборка без ключа компилируется, но
/// **отказывается обновляться**: тихо принимать неподписанные обновления
/// нельзя.
///
/// Общая функция для службы и для `classos-updater.exe`: два разных
/// определения одного доверенного ключа рано или поздно разошлись бы.
pub fn publisher_key() -> Result<[u8; 32], UpdateError> {
    let Some(hex) = option_env!("CLASSOS_PUBLISHER_KEY_HEX") else {
        return Err(UpdateError::MissingPublisherKey);
    };
    if hex.len() != 64 {
        return Err(UpdateError::InvalidPublisherKey);
    }
    let mut bytes = [0_u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| UpdateError::InvalidPublisherKey)?;
    }
    Ok(bytes)
}

/// Проверяет манифест до любой загрузки файла.
///
/// Порядок обязателен: подпись проверяется первой, потому что все остальные
/// поля манифеста доверенными становятся только после неё.
pub fn evaluate_manifest(
    publisher_public_key: &[u8; 32],
    manifest: &UpdateManifest,
    current_version: &str,
    channel: Channel,
) -> Result<UpdateDecision, UpdateError> {
    let verifying_key = VerifyingKey::from_bytes(publisher_public_key)
        .map_err(|_| UpdateError::InvalidPublisherKey)?;
    verifying_key
        .verify(
            &manifest_payload(manifest),
            &Signature::from_bytes(&manifest.signature),
        )
        .map_err(|_| UpdateError::InvalidSignature)?;

    if manifest.release_channel != channel.as_str() {
        return Err(UpdateError::ChannelMismatch);
    }

    let parse = |value: &str| {
        Version::parse(value).map_err(|_| UpdateError::InvalidVersion(value.to_owned()))
    };
    let new_version = parse(&manifest.version)?;
    let current = parse(current_version)?;
    let minimum = parse(&manifest.minimum_supported_version)?;

    if current < minimum {
        return Err(UpdateError::UnsupportedCurrentVersion);
    }
    if new_version <= current {
        return Ok(UpdateDecision::AlreadyCurrent);
    }
    Ok(UpdateDecision::Apply)
}

/// Проверяет содержимое загруженного файла.
///
/// Хеш проверяется **до** установки: файл, не совпавший с манифестом, не
/// должен доходить до файловой системы службы (инвариант T8 §12.3).
pub fn verify_payload(manifest: &UpdateManifest, bytes: &[u8]) -> Result<(), UpdateError> {
    let digest = Sha256::digest(bytes);
    let actual = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual.eq_ignore_ascii_case(&manifest.sha256) {
        Ok(())
    } else {
        Err(UpdateError::HashMismatch)
    }
}

/// Операции над установленными файлами агента.
///
/// Service не заменяет сам себя вживую (§8.4): реализация этого трейта живёт
/// в отдельном процессе `classos-updater.exe`.
pub trait BinaryStore {
    /// Сохраняет текущие бинарники, чтобы к ним можно было вернуться.
    fn backup(&self) -> Result<(), UpdateError>;
    /// Заменяет бинарники содержимым обновления.
    fn install(&self, payload: &[u8]) -> Result<(), UpdateError>;
    /// Возвращает сохранённые бинарники на место.
    fn restore(&self) -> Result<(), UpdateError>;
}

/// Проверка работоспособности агента после установки.
pub trait HealthCheck {
    fn is_healthy(&self) -> Result<(), String>;
}

/// Результат применения обновления.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    Installed {
        version: String,
    },
    /// Обновление установилось, но health check не прошёл — выполнен откат.
    RolledBack {
        version: String,
        reason: String,
    },
}

/// Устанавливает проверенное обновление с обязательным откатом.
///
/// Вызывается только после `evaluate_manifest` и `verify_payload`: этот
/// уровень отвечает за файлы и откат, а не за доверие к манифесту.
pub fn install_verified<S: BinaryStore, H: HealthCheck>(
    store: &S,
    health: &H,
    manifest: &UpdateManifest,
    payload: &[u8],
) -> Result<UpdateOutcome, UpdateError> {
    store.backup()?;
    if let Err(error) = store.install(payload) {
        // Установка могла заменить часть файлов — возвращаем сохранённые.
        store.restore()?;
        return Err(error);
    }
    match health.is_healthy() {
        Ok(()) => Ok(UpdateOutcome::Installed {
            version: manifest.version.clone(),
        }),
        Err(reason) => {
            store.restore()?;
            Ok(UpdateOutcome::RolledBack {
                version: manifest.version.clone(),
                reason,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use std::cell::RefCell;

    fn publisher() -> SigningKey {
        SigningKey::from_bytes(&[3_u8; 32])
    }

    fn payload_bytes() -> Vec<u8> {
        b"classos-agent-binary".to_vec()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn manifest(version: &str, channel: &str) -> UpdateManifest {
        let mut value = UpdateManifest {
            version: version.to_owned(),
            url: "https://updates.classos.example/agent.bin".to_owned(),
            sha256: sha256_hex(&payload_bytes()),
            signature: [0_u8; 64],
            minimum_supported_version: "0.1.0".to_owned(),
            release_channel: channel.to_owned(),
        };
        value.signature = sign_manifest(&publisher(), &value);
        value
    }

    fn public() -> [u8; 32] {
        publisher().verifying_key().to_bytes()
    }

    #[test]
    fn newer_signed_manifest_is_applied() {
        assert_eq!(
            evaluate_manifest(
                &public(),
                &manifest("0.2.0", "stable"),
                "0.1.0",
                Channel::Stable
            ),
            Ok(UpdateDecision::Apply)
        );
    }

    #[test]
    fn same_or_older_version_is_not_applied() {
        assert_eq!(
            evaluate_manifest(
                &public(),
                &manifest("0.1.0", "stable"),
                "0.1.0",
                Channel::Stable
            ),
            Ok(UpdateDecision::AlreadyCurrent)
        );
        assert_eq!(
            evaluate_manifest(
                &public(),
                &manifest("0.1.0", "stable"),
                "0.5.0",
                Channel::Stable
            ),
            Ok(UpdateDecision::AlreadyCurrent)
        );
    }

    #[test]
    fn tampered_manifest_is_rejected() {
        let mut forged = manifest("0.2.0", "stable");
        forged.url = "https://evil.example/payload.bin".to_owned();
        assert_eq!(
            evaluate_manifest(&public(), &forged, "0.1.0", Channel::Stable),
            Err(UpdateError::InvalidSignature)
        );

        let mut forged = manifest("0.2.0", "stable");
        forged.sha256 = sha256_hex(b"other");
        assert_eq!(
            evaluate_manifest(&public(), &forged, "0.1.0", Channel::Stable),
            Err(UpdateError::InvalidSignature)
        );
    }

    #[test]
    fn manifest_from_another_publisher_is_rejected() {
        let other = SigningKey::from_bytes(&[8_u8; 32]);
        let mut value = manifest("0.2.0", "stable");
        value.signature = sign_manifest(&other, &value);
        assert_eq!(
            evaluate_manifest(&public(), &value, "0.1.0", Channel::Stable),
            Err(UpdateError::InvalidSignature)
        );
    }

    #[test]
    fn manifest_for_another_channel_is_rejected() {
        // Устройство на stable не должно принимать beta-сборку.
        assert_eq!(
            evaluate_manifest(
                &public(),
                &manifest("0.2.0", "beta"),
                "0.1.0",
                Channel::Stable
            ),
            Err(UpdateError::ChannelMismatch)
        );
    }

    #[test]
    fn too_old_agent_refuses_the_update() {
        let mut value = UpdateManifest {
            minimum_supported_version: "0.4.0".to_owned(),
            ..manifest("0.5.0", "stable")
        };
        value.signature = sign_manifest(&publisher(), &value);
        assert_eq!(
            evaluate_manifest(&public(), &value, "0.3.0", Channel::Stable),
            Err(UpdateError::UnsupportedCurrentVersion)
        );
    }

    #[test]
    fn payload_hash_must_match_the_manifest() {
        let value = manifest("0.2.0", "stable");
        assert_eq!(verify_payload(&value, &payload_bytes()), Ok(()));
        assert_eq!(
            verify_payload(&value, b"tampered"),
            Err(UpdateError::HashMismatch)
        );
    }

    #[derive(Default)]
    struct FakeStore {
        installed: RefCell<Vec<u8>>,
        backup: RefCell<Option<Vec<u8>>>,
        restored: RefCell<bool>,
        install_fails: bool,
    }

    impl BinaryStore for FakeStore {
        fn backup(&self) -> Result<(), UpdateError> {
            *self.backup.borrow_mut() = Some(self.installed.borrow().clone());
            Ok(())
        }
        fn install(&self, payload: &[u8]) -> Result<(), UpdateError> {
            if self.install_fails {
                // Половина файлов уже заменена — типичный частичный сбой.
                *self.installed.borrow_mut() = b"partial".to_vec();
                return Err(UpdateError::Install("диск заполнен".to_owned()));
            }
            *self.installed.borrow_mut() = payload.to_vec();
            Ok(())
        }
        fn restore(&self) -> Result<(), UpdateError> {
            let backup = self
                .backup
                .borrow()
                .clone()
                .ok_or_else(|| UpdateError::Rollback("нет сохранённой копии".to_owned()))?;
            *self.installed.borrow_mut() = backup;
            *self.restored.borrow_mut() = true;
            Ok(())
        }
    }

    struct FixedHealth(Result<(), String>);
    impl HealthCheck for FixedHealth {
        fn is_healthy(&self) -> Result<(), String> {
            self.0.clone()
        }
    }

    #[test]
    fn healthy_update_stays_installed() {
        let store = FakeStore {
            installed: RefCell::new(b"old".to_vec()),
            ..FakeStore::default()
        };
        let outcome = install_verified(
            &store,
            &FixedHealth(Ok(())),
            &manifest("0.2.0", "stable"),
            &payload_bytes(),
        )
        .unwrap();

        assert_eq!(
            outcome,
            UpdateOutcome::Installed {
                version: "0.2.0".to_owned()
            }
        );
        assert_eq!(*store.installed.borrow(), payload_bytes());
        assert!(!*store.restored.borrow());
    }

    #[test]
    fn failed_health_check_rolls_back_to_previous_binaries() {
        let store = FakeStore {
            installed: RefCell::new(b"old".to_vec()),
            ..FakeStore::default()
        };
        let outcome = install_verified(
            &store,
            &FixedHealth(Err("служба не стартовала".to_owned())),
            &manifest("0.2.0", "stable"),
            &payload_bytes(),
        )
        .unwrap();

        assert!(matches!(outcome, UpdateOutcome::RolledBack { .. }));
        // Главное: устройство работает на прежней версии, а не сломано.
        assert_eq!(*store.installed.borrow(), b"old".to_vec());
        assert!(*store.restored.borrow());
    }

    #[test]
    fn partial_install_failure_also_rolls_back() {
        let store = FakeStore {
            installed: RefCell::new(b"old".to_vec()),
            install_fails: true,
            ..FakeStore::default()
        };
        let error = install_verified(
            &store,
            &FixedHealth(Ok(())),
            &manifest("0.2.0", "stable"),
            &payload_bytes(),
        )
        .unwrap_err();

        assert!(matches!(error, UpdateError::Install(_)));
        assert_eq!(*store.installed.borrow(), b"old".to_vec());
    }

    #[test]
    fn manifest_json_round_trip_preserves_signature() {
        let value = manifest("0.2.0", "stable");
        let text = serde_json::to_string(&value).unwrap();
        assert_eq!(parse_manifest(&text).unwrap(), value);
    }

    #[test]
    fn manifest_with_malformed_signature_is_rejected() {
        assert!(parse_manifest(r#"{"version":"0.2.0","url":"u","sha256":"a","signature":"00","minimum_supported_version":"0.1.0","release_channel":"stable"}"#).is_err());
    }

    #[test]
    fn channel_names_round_trip() {
        for channel in [Channel::Stable, Channel::Beta, Channel::Canary] {
            assert_eq!(Channel::parse(channel.as_str()), Some(channel));
        }
        assert_eq!(Channel::parse("nightly"), None);
    }
}
