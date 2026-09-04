//! Переносимый продуктовый слой политик T6 без Windows registry/GPO деталей.

use std::collections::BTreeSet;

pub const CLASSOS_BINARIES: &[&str] = &[
    "classos-service.exe",
    "classos-session.exe",
    "classos-updater.exe",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPolicy {
    pub policy_name: String,
    pub allowed_executables: BTreeSet<String>,
    pub allowed_urls: BTreeSet<String>,
    pub restrictions: Restrictions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
}
