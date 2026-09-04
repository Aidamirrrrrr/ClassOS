//! Переносимый продуктовый слой политик T6 без Windows registry/GPO деталей.
//!
//! Слой оперирует только продуктовыми понятиями: идентификаторами приложений,
//! URL и набором системных ограничений. Трансляция в AppLocker, registry и
//! browser policy живёт за трейтом [`PolicyProvider`] (ADR-0006, ADR-0014).

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Собственные бинарники ClassOS, которые политика обязана разрешать всегда.
///
/// Пропуск этого шага — критический баг: ошибочная политика заблокировала бы
/// сам management layer и лишила бы возможности её исправить (spec T6 §8).
pub const CLASSOS_BINARIES: &[&str] = &[
    "classos-service.exe",
    "classos-session.exe",
    "classos-updater.exe",
];

/// Версия формата политики, передаваемой по сети.
pub const POLICY_DOCUMENT_VERSION: u32 = 1;

/// Версия формата состояния политики на диске.
pub const POLICY_STATE_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Продуктовая модель
// ---------------------------------------------------------------------------

/// Продуктовая политика урока. Содержит идентификаторы приложений каталога,
/// а не пути к исполняемым файлам: разрешение в конкретный бинарник — задача
/// устройства, а не Teacher Console.
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
    /// Слой сужает список приложений до своего, а не дополняет нижние слои.
    /// Используется Focus Mode; обычные слои складываются.
    #[serde(default)]
    pub restrict_to_allowed: bool,
}

/// Слои политики. Модель обязана существовать в этом виде с самого начала,
/// даже пока Branch/Room — статичные локальные заглушки (spec T6 §10).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyLayers {
    pub base: LessonPolicy,
    pub branch: LessonPolicy,
    pub room: LessonPolicy,
    pub lesson: LessonPolicy,
    pub temporary_override: Option<LessonPolicy>,
}

impl PolicyLayers {
    /// Детерминированный расчёт EffectivePolicy из слоёв.
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

    /// Focus Mode — не отдельный механизм, а временный слой поверх остальных
    /// (spec T6 §11). Он сужает список приложений до явно разрешённых.
    pub fn enable_focus(&mut self, allowed_application_ids: impl IntoIterator<Item = String>) {
        self.temporary_override = Some(LessonPolicy {
            name: FOCUS_LAYER_NAME.to_owned(),
            allowed_applications: allowed_application_ids.into_iter().collect(),
            restrict_to_allowed: true,
            ..Default::default()
        });
    }

    pub fn disable_focus(&mut self) {
        self.temporary_override = None;
    }

    pub fn focus_is_active(&self) -> bool {
        self.temporary_override.is_some()
    }
}

/// Имя временного слоя Focus Mode.
pub const FOCUS_LAYER_NAME: &str = "Focus Mode";

fn merge_policy(target: &mut LessonPolicy, layer: &LessonPolicy) {
    if !layer.name.is_empty() {
        target.name = layer.name.clone();
    }
    if layer.restrict_to_allowed {
        // Focus сужает набор, а не расширяет его: иначе "разрешить только
        // VS Code" оставило бы разрешёнными все приложения нижних слоёв.
        target
            .allowed_applications
            .clone_from(&layer.allowed_applications);
    } else {
        target
            .allowed_applications
            .extend(layer.allowed_applications.iter().cloned());
    }
    target
        .allowed_urls
        .extend(layer.allowed_urls.iter().cloned());
    target.block_settings |= layer.block_settings;
    target.block_powershell |= layer.block_powershell;
    target.block_cmd |= layer.block_cmd;
    target.block_store |= layer.block_store;
    target.block_personalization |= layer.block_personalization;
}

// ---------------------------------------------------------------------------
// Каталог приложений
// ---------------------------------------------------------------------------

/// Разрешение продуктового идентификатора приложения в имя исполняемого файла.
///
/// Каталог живёт на устройстве: Teacher Console присылает `"vscode"`, но
/// никогда не путь и не командную строку (тот же принцип, что в T5).
pub trait ApplicationCatalog {
    fn resolve(&self, application_id: &str) -> Option<String>;
}

// ---------------------------------------------------------------------------
// Компиляция
// ---------------------------------------------------------------------------

/// Результат компиляции: всё ещё без Windows-деталей, но уже в терминах
/// исполняемых файлов и ограничений, понятных провайдеру.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPolicy {
    pub policy_name: String,
    /// Имена исполняемых файлов, включая обязательные бинарники ClassOS.
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

/// Компилирует продуктовую политику в набор правил.
///
/// Неизвестный идентификатор приложения — ошибка компиляции, а не молча
/// пропущенное правило: иначе опечатка в профиле тихо оставила бы приложение
/// заблокированным без объяснения.
pub fn compile(
    policy: &LessonPolicy,
    catalog: &dyn ApplicationCatalog,
) -> Result<CompiledPolicy, PolicyError> {
    if policy.name.trim().is_empty() {
        return Err(PolicyError::Invalid("отсутствует имя политики".to_owned()));
    }
    let mut allowed_executables = BTreeSet::new();
    for application_id in &policy.allowed_applications {
        let executable = catalog.resolve(application_id).ok_or_else(|| {
            PolicyError::Invalid(format!(
                "приложение отсутствует в каталоге устройства: {application_id}"
            ))
        })?;
        allowed_executables.insert(executable);
    }
    for url in &policy.allowed_urls {
        if !is_valid_policy_url(url) {
            return Err(PolicyError::Invalid(format!("некорректный URL: {url}")));
        }
    }
    // Auto-allow выполняется после разрешения каталога и до применения — так
    // ни один путь исполнения не может его пропустить (spec T6 §8).
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

/// Хост для browser policy: без схемы, пробелов и wildcard-мусора.
fn is_valid_policy_url(url: &str) -> bool {
    !url.is_empty()
        && url.len() <= 512
        && !url.contains(char::is_whitespace)
        && url
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+,;=%".contains(value))
}

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("некорректная политика: {0}")]
    Invalid(String),
    #[error("enforcement недоступен на устройстве: {0}")]
    Unsupported(String),
    #[error("provider: {0}")]
    Provider(String),
    #[error("хранилище policy: {0}")]
    Storage(String),
}

impl PolicyError {
    /// Стабильный код ошибки для протокола: Teacher Console показывает
    /// продуктовое сообщение, а не текст Windows.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) => "POLICY_INVALID",
            Self::Unsupported(_) => "POLICY_UNSUPPORTED",
            Self::Provider(_) => "POLICY_APPLY_FAILED",
            Self::Storage(_) => "POLICY_STORAGE_FAILED",
        }
    }
}

// ---------------------------------------------------------------------------
// Сетевой документ
// ---------------------------------------------------------------------------

/// Версионированный контейнер, который Teacher Console кладёт в
/// `ApplyPolicy.compiled_policy`.
///
/// По сети передаётся именно продуктовая политика, а не готовые правила:
/// компиляция и обязательный auto-allow выполняются на устройстве, поэтому их
/// нельзя обойти со стороны сети.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDocument {
    pub version: u32,
    pub policy: LessonPolicy,
}

impl PolicyDocument {
    pub fn new(policy: LessonPolicy) -> Self {
        Self {
            version: POLICY_DOCUMENT_VERSION,
            policy,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, PolicyError> {
        serde_json::to_vec(self).map_err(|error| PolicyError::Invalid(error.to_string()))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, PolicyError> {
        let document: Self = serde_json::from_slice(bytes)
            .map_err(|error| PolicyError::Invalid(error.to_string()))?;
        if document.version != POLICY_DOCUMENT_VERSION {
            return Err(PolicyError::Invalid(format!(
                "неподдерживаемая версия документа политики: {}",
                document.version
            )));
        }
        Ok(document)
    }
}

// ---------------------------------------------------------------------------
// Состояние на диске
// ---------------------------------------------------------------------------

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
        // Запись через временный файл: обрыв питания посреди сохранения не
        // должен оставить нечитаемое состояние политики.
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, text)
            .map_err(|error| PolicyError::Storage(error.to_string()))?;
        std::fs::rename(&temporary, path).map_err(|error| PolicyError::Storage(error.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Что устройство умеет на самом деле.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    /// Enforcement доступен полностью.
    Enforced,
    /// Enforcement недоступен; причина показывается администратору.
    Unavailable(String),
}

/// Абстракция enforcement-механизма устройства.
///
/// Реализация обязана быть honest: если применить политику нельзя, это
/// сообщается до Apply, а не подменяется видимостью защиты (ADR-0014).
pub trait PolicyProvider {
    type Snapshot;

    /// Проверка возможностей устройства до любой попытки применения.
    fn check_support(&self) -> Capability;
    fn validate(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError>;
    fn snapshot(&self) -> Result<Self::Snapshot, PolicyError>;
    fn apply(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError>;
    fn verify(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError>;
    fn rollback(&self, snapshot: &Self::Snapshot) -> Result<(), PolicyError>;
}

/// Обязательная последовательность безопасного применения (spec T6 §6).
///
/// После snapshot любая ошибка гарантированно запускает rollback: устройство
/// никогда не остаётся в частично применённом состоянии.
pub fn apply_safely<P: PolicyProvider>(
    provider: &P,
    compiled: &CompiledPolicy,
) -> Result<P::Snapshot, PolicyError> {
    if let Capability::Unavailable(reason) = provider.check_support() {
        return Err(PolicyError::Unsupported(reason));
    }
    provider.validate(compiled)?;
    let snapshot = provider.snapshot()?;
    if let Err(error) = provider
        .apply(compiled)
        .and_then(|()| provider.verify(compiled))
    {
        provider.rollback(&snapshot)?;
        return Err(error);
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct TestCatalog;
    impl ApplicationCatalog for TestCatalog {
        fn resolve(&self, application_id: &str) -> Option<String> {
            match application_id {
                "vscode" => Some("Code.exe".to_owned()),
                "chrome" => Some("chrome.exe".to_owned()),
                "python" => Some("python.exe".to_owned()),
                _ => None,
            }
        }
    }

    fn policy(name: &str, applications: &[&str]) -> LessonPolicy {
        LessonPolicy {
            name: name.to_owned(),
            allowed_applications: applications.iter().map(|v| (*v).to_owned()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn compiler_always_allows_classos_binaries() {
        let compiled = compile(&policy("Python", &["python"]), &TestCatalog).unwrap();
        assert!(
            CLASSOS_BINARIES
                .iter()
                .all(|name| compiled.allowed_executables.contains(*name))
        );
    }

    #[test]
    fn compiler_resolves_application_ids_to_executables() {
        let compiled = compile(&policy("Python", &["vscode"]), &TestCatalog).unwrap();
        assert!(compiled.allowed_executables.contains("Code.exe"));
        // Продуктовый идентификатор не должен просочиться в правила.
        assert!(!compiled.allowed_executables.contains("vscode"));
    }

    #[test]
    fn compiler_rejects_application_outside_catalog() {
        let error = compile(&policy("Bad", &["telegram"]), &TestCatalog).unwrap_err();
        assert_eq!(error.code(), "POLICY_INVALID");
    }

    #[test]
    fn compiler_rejects_malformed_url() {
        let mut value = policy("Bad", &[]);
        value.allowed_urls.insert("docs python org".to_owned());
        assert!(compile(&value, &TestCatalog).is_err());
    }

    #[test]
    fn focus_narrows_instead_of_extending() {
        let mut layers = PolicyLayers {
            base: policy("Base", &["chrome", "python"]),
            ..Default::default()
        };
        layers.enable_focus(["vscode".to_owned()]);
        let effective = layers.effective();
        assert!(effective.allowed_applications.contains("vscode"));
        // Смысл Focus — "только это приложение", иначе кнопка бесполезна.
        assert!(!effective.allowed_applications.contains("chrome"));

        layers.disable_focus();
        let effective = layers.effective();
        assert!(effective.allowed_applications.contains("chrome"));
        assert!(!effective.allowed_applications.contains("vscode"));
    }

    #[test]
    fn layering_is_deterministic_across_levels() {
        let layers = PolicyLayers {
            base: policy("Base", &["chrome"]),
            branch: LessonPolicy {
                block_store: true,
                ..policy("Branch", &["python"])
            },
            room: LessonPolicy {
                block_cmd: true,
                ..Default::default()
            },
            lesson: policy("Python", &["vscode"]),
            temporary_override: None,
        };
        let effective = layers.effective();
        assert_eq!(effective.name, "Python");
        assert_eq!(effective.allowed_applications.len(), 3);
        assert!(effective.block_store && effective.block_cmd);
        assert!(!effective.block_powershell);
    }

    struct ScriptedProvider {
        capability: Capability,
        verify_fails: bool,
        rolled_back: Cell<bool>,
        applied: Cell<bool>,
    }

    impl ScriptedProvider {
        fn healthy() -> Self {
            Self {
                capability: Capability::Enforced,
                verify_fails: false,
                rolled_back: Cell::new(false),
                applied: Cell::new(false),
            }
        }
    }

    impl PolicyProvider for ScriptedProvider {
        type Snapshot = String;
        fn check_support(&self) -> Capability {
            self.capability.clone()
        }
        fn validate(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError> {
            compiled
                .allowed_executables
                .contains("classos-service.exe")
                .then_some(())
                .ok_or(PolicyError::Invalid("нет allow для ClassOS".to_owned()))
        }
        fn snapshot(&self) -> Result<String, PolicyError> {
            Ok("snapshot".to_owned())
        }
        fn apply(&self, _: &CompiledPolicy) -> Result<(), PolicyError> {
            self.applied.set(true);
            Ok(())
        }
        fn verify(&self, _: &CompiledPolicy) -> Result<(), PolicyError> {
            if self.verify_fails {
                return Err(PolicyError::Provider("verify failed".to_owned()));
            }
            Ok(())
        }
        fn rollback(&self, _: &String) -> Result<(), PolicyError> {
            self.rolled_back.set(true);
            Ok(())
        }
    }

    #[test]
    fn verification_failure_rolls_back_snapshot() {
        let provider = ScriptedProvider {
            verify_fails: true,
            ..ScriptedProvider::healthy()
        };
        let compiled = compile(&policy("Test", &[]), &TestCatalog).unwrap();
        assert!(apply_safely(&provider, &compiled).is_err());
        assert!(provider.rolled_back.get());
    }

    #[test]
    fn unsupported_device_never_reaches_apply() {
        let provider = ScriptedProvider {
            capability: Capability::Unavailable("нет службы AppIDSvc".to_owned()),
            ..ScriptedProvider::healthy()
        };
        let compiled = compile(&policy("Test", &[]), &TestCatalog).unwrap();
        let error = apply_safely(&provider, &compiled).unwrap_err();
        assert_eq!(error.code(), "POLICY_UNSUPPORTED");
        // Главное: устройство не осталось в частично применённом состоянии.
        assert!(!provider.applied.get());
        assert!(!provider.rolled_back.get());
    }

    #[test]
    fn document_round_trip_rejects_foreign_version() {
        let document = PolicyDocument::new(policy("Python", &["vscode"]));
        let bytes = document.encode().unwrap();
        assert_eq!(PolicyDocument::decode(&bytes).unwrap(), document);

        let foreign = br#"{"version":99,"policy":{"name":"x","allowed_applications":[],"allowed_urls":[],"block_settings":false,"block_powershell":false,"block_cmd":false,"block_store":false,"block_personalization":false,"restrict_to_allowed":false}}"#;
        assert!(PolicyDocument::decode(foreign).is_err());
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
