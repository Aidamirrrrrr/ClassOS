//! Переносимый продуктовый слой политик T6 без Windows registry/GPO деталей.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const CLASSOS_BINARIES: &[&str] = &[
    "classos-service.exe",
    "classos-session.exe",
    "classos-updater.exe",
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LessonPolicy {
    pub name: String,
    pub allowed_applications: BTreeSet<String>,
    pub allowed_urls: BTreeSet<String>,
    pub block_settings: bool,
    pub block_powershell: bool,
    pub block_cmd: bool,
    pub block_store: bool,
    pub block_personalization: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyLayers {
    pub base: LessonPolicy,
    pub branch: LessonPolicy,
    pub room: LessonPolicy,
    pub lesson: LessonPolicy,
    pub temporary_override: Option<LessonPolicy>,
}

impl PolicyLayers {
    pub fn effective(&self) -> LessonPolicy {
        let mut result = self.base.clone();
        for layer in [&self.branch, &self.room, &self.lesson] {
            merge_policy(&mut result, layer);
        }
        if let Some(layer) = &self.temporary_override {
            merge_policy(&mut result, layer);
        }
        result
    }

    pub fn enable_focus(&mut self, allowed_application_ids: impl IntoIterator<Item = String>) {
        self.temporary_override = Some(LessonPolicy {
            name: "Focus Mode".to_owned(),
            allowed_applications: allowed_application_ids.into_iter().collect(),
            ..Default::default()
        });
    }

    pub fn disable_focus(&mut self) {
        self.temporary_override = None;
    }
}

fn merge_policy(target: &mut LessonPolicy, layer: &LessonPolicy) {
    if !layer.name.is_empty() {
        target.name = layer.name.clone();
    }
    target
        .allowed_applications
        .extend(layer.allowed_applications.iter().cloned());
    target
        .allowed_urls
        .extend(layer.allowed_urls.iter().cloned());
    target.block_settings |= layer.block_settings;
    target.block_powershell |= layer.block_powershell;
    target.block_cmd |= layer.block_cmd;
    target.block_store |= layer.block_store;
    target.block_personalization |= layer.block_personalization;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPolicy {
    pub policy_name: String,
    pub allowed_executables: BTreeSet<String>,
    pub allowed_urls: BTreeSet<String>,
    pub restrictions: Restrictions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restrictions {
    pub settings: bool,
    pub powershell: bool,
    pub cmd: bool,
    pub store: bool,
    pub personalization: bool,
}

pub fn compile(policy: &LessonPolicy) -> Result<CompiledPolicy, PolicyError> {
    if policy.name.trim().is_empty() {
        return Err(PolicyError::Invalid("отсутствует имя политики"));
    }
    let mut allowed_executables = policy.allowed_applications.clone();
    allowed_executables.extend(CLASSOS_BINARIES.iter().map(|value| (*value).to_owned()));
    Ok(CompiledPolicy {
        policy_name: policy.name.clone(),
        allowed_executables,
        allowed_urls: policy.allowed_urls.clone(),
        restrictions: Restrictions {
            settings: policy.block_settings,
            powershell: policy.block_powershell,
            cmd: policy.block_cmd,
            store: policy.block_store,
            personalization: policy.block_personalization,
        },
    })
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("некорректная политика: {0}")]
    Invalid(&'static str),
    #[error("provider: {0}")]
    Provider(String),
    #[error("хранилище policy: {0}")]
    Storage(String),
}

pub const POLICY_STATE_VERSION: u32 = 1;

/// Сохраняем только продуктовую модель, никогда не registry/GPO-детали provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedPolicyState {
    pub version: u32,
    pub layers: PolicyLayers,
    pub active_snapshot_id: Option<String>,
}

impl PersistedPolicyState {
    pub fn load(path: &Path) -> Result<Self, PolicyError> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let state: Self = toml::from_str(&text)
                    .map_err(|error| PolicyError::Storage(error.to_string()))?;
                (state.version == POLICY_STATE_VERSION)
                    .then_some(state)
                    .ok_or(PolicyError::Storage(
                        "неподдерживаемая версия policy state".to_owned(),
                    ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                version: POLICY_STATE_VERSION,
                ..Default::default()
            }),
            Err(error) => Err(PolicyError::Storage(error.to_string())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), PolicyError> {
        let parent = path.parent().ok_or_else(|| {
            PolicyError::Storage("у policy state нет родительского каталога".to_owned())
        })?;
        std::fs::create_dir_all(parent).map_err(|error| PolicyError::Storage(error.to_string()))?;
        let text =
            toml::to_string(self).map_err(|error| PolicyError::Storage(error.to_string()))?;
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, text)
            .map_err(|error| PolicyError::Storage(error.to_string()))?;
        std::fs::rename(&temporary, path).map_err(|error| PolicyError::Storage(error.to_string()))
    }
}

pub trait PolicyProvider {
    type Snapshot;
    fn validate(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError>;
    fn snapshot(&self) -> Result<Self::Snapshot, PolicyError>;
    fn apply(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError>;
    fn verify(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError>;
    fn rollback(&self, snapshot: &Self::Snapshot) -> Result<(), PolicyError>;
}

/// После snapshot любая ошибка гарантированно запускает rollback.
pub fn apply_safely<P: PolicyProvider>(
    provider: &P,
    compiled: &CompiledPolicy,
) -> Result<(), PolicyError> {
    provider.validate(compiled)?;
    let snapshot = provider.snapshot()?;
    if let Err(error) = provider
        .apply(compiled)
        .and_then(|()| provider.verify(compiled))
    {
        provider.rollback(&snapshot)?;
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn compiler_always_allows_classos_binaries() {
        let compiled = compile(&LessonPolicy {
            name: "Python".to_owned(),
            ..Default::default()
        })
        .unwrap();
        assert!(
            CLASSOS_BINARIES
                .iter()
                .all(|name| compiled.allowed_executables.contains(*name))
        );
    }

    #[test]
    fn focus_is_temporary_layer() {
        let mut layers = PolicyLayers {
            base: LessonPolicy {
                name: "Base".to_owned(),
                allowed_applications: ["chrome".to_owned()].into_iter().collect(),
                ..Default::default()
            },
            ..Default::default()
        };
        layers.enable_focus(["vscode".to_owned()]);
        assert!(layers.effective().allowed_applications.contains("vscode"));
        layers.disable_focus();
        assert!(layers.effective().allowed_applications.contains("chrome"));
        assert!(!layers.effective().allowed_applications.contains("vscode"));
    }

    struct FailingProvider {
        rolled_back: Cell<bool>,
    }
    impl PolicyProvider for FailingProvider {
        type Snapshot = ();
        fn validate(&self, _: &CompiledPolicy) -> Result<(), PolicyError> {
            Ok(())
        }
        fn snapshot(&self) -> Result<(), PolicyError> {
            Ok(())
        }
        fn apply(&self, _: &CompiledPolicy) -> Result<(), PolicyError> {
            Ok(())
        }
        fn verify(&self, _: &CompiledPolicy) -> Result<(), PolicyError> {
            Err(PolicyError::Provider("verify failed".to_owned()))
        }
        fn rollback(&self, _: &()) -> Result<(), PolicyError> {
            self.rolled_back.set(true);
            Ok(())
        }
    }
    #[test]
    fn verification_failure_rolls_back_snapshot() {
        let provider = FailingProvider {
            rolled_back: Cell::new(false),
        };
        let compiled = compile(&LessonPolicy {
            name: "Test".to_owned(),
            ..Default::default()
        })
        .unwrap();
        assert!(apply_safely(&provider, &compiled).is_err());
        assert!(provider.rolled_back.get());
    }

    #[test]
    fn state_round_trip_keeps_focus_and_snapshot() {
        let directory =
            std::env::temp_dir().join(format!("classos-policy-test-{}", std::process::id()));
        let path = directory.join("policy.toml");
        let mut state = PersistedPolicyState {
            version: POLICY_STATE_VERSION,
            active_snapshot_id: Some("snapshot-1".to_owned()),
            ..Default::default()
        };
        state.layers.enable_focus(["vscode".to_owned()]);
        state.save(&path).unwrap();
        let loaded = PersistedPolicyState::load(&path).unwrap();
        assert_eq!(loaded.active_snapshot_id.as_deref(), Some("snapshot-1"));
        assert!(
            loaded
                .layers
                .effective()
                .allowed_applications
                .contains("vscode")
        );
        let _ = std::fs::remove_dir_all(directory);
    }
}
