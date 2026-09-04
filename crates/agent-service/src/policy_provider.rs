//! Windows-провайдер enforcement для T6: AppLocker + policy-ключи реестра.
//!
//! Здесь и только здесь продуктовые ограничения превращаются в конкретные
//! механизмы Windows (ADR-0006, ADR-0014). Выше по стеку — `agent-core::policy`
//! и `policy-engine` — Windows-деталей нет вовсе, а Teacher Console не видит их
//! тем более (инвариант X).

use std::path::PathBuf;

use agent_core::policy::SnapshotStore;
use policy_engine::{Capability, CompiledPolicy, PolicyError, PolicyProvider};
use serde::{Deserialize, Serialize};
use windows_platform::applocker::{self, PolicyDecision};
use windows_platform::registry::{self, RegistryData};

/// Ограничение "Настройки Windows".
const EXPLORER_POLICIES: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer";
const NO_CONTROL_PANEL: &str = "NoControlPanel";

/// Ограничение "Microsoft Store".
const STORE_POLICIES: &str = r"SOFTWARE\Policies\Microsoft\WindowsStore";
const REMOVE_WINDOWS_STORE: &str = "RemoveWindowsStore";

/// Ограничение "персонализация".
const ACTIVE_DESKTOP_POLICIES: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\ActiveDesktop";
const NO_CHANGING_WALLPAPER: &str = "NoChangingWallPaper";

/// Enterprise-policy браузеров: только официальные ключи, без MITM (spec §12).
const BROWSER_POLICY_KEYS: &[(&str, &str)] = &[
    (
        r"SOFTWARE\Policies\Google\Chrome\URLAllowlist",
        r"SOFTWARE\Policies\Google\Chrome\URLBlocklist",
    ),
    (
        r"SOFTWARE\Policies\Microsoft\Edge\URLAllowlist",
        r"SOFTWARE\Policies\Microsoft\Edge\URLBlocklist",
    ),
];

/// Сколько пронумерованных значений URL-списков мы за собой убираем.
/// Snapshot охватывает ровно этот диапазон, поэтому откат не может оставить
/// "хвост" от более длинного предыдущего списка.
const MAX_URL_RULES: usize = 16;

/// Исполняемые файлы, которые скрываются за продуктовыми ограничениями.
const CMD_EXECUTABLES: &[&str] = &["cmd.exe"];
const POWERSHELL_EXECUTABLES: &[&str] = &["powershell.exe", "powershell_ise.exe", "pwsh.exe"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum StoredValue {
    Dword(u32),
    Text(String),
}

impl From<&RegistryData> for StoredValue {
    fn from(value: &RegistryData) -> Self {
        match value {
            RegistryData::Dword(number) => Self::Dword(*number),
            RegistryData::Text(text) => Self::Text(text.clone()),
        }
    }
}

impl From<&StoredValue> for RegistryData {
    fn from(value: &StoredValue) -> Self {
        match value {
            StoredValue::Dword(number) => Self::Dword(*number),
            StoredValue::Text(text) => Self::Text(text.clone()),
        }
    }
}

/// Прежнее состояние одного значения. `None` означает "значения не было" —
/// откат обязан отличать это от "значение было равно нулю".
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryBackup {
    subkey: String,
    name: String,
    value: Option<StoredValue>,
}

/// Снимок состояния устройства до применения политики.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsPolicySnapshot {
    pub id: String,
    /// Прежняя локальная политика AppLocker целиком.
    applocker_xml: String,
    registry: Vec<RegistryBackup>,
}

pub struct WindowsPolicyProvider {
    snapshot_dir: PathBuf,
    workspace: PathBuf,
    /// Путь к собственному бинарнику: на нём проверяется, что политика не
    /// заблокирует сам ClassOS (spec §6, §8).
    self_executable: PathBuf,
}

impl WindowsPolicyProvider {
    pub fn new() -> Result<Self, PolicyError> {
        let self_executable = std::env::current_exe()
            .map_err(|error| PolicyError::Provider(format!("current_exe: {error}")))?;
        let provider = Self {
            snapshot_dir: agent_core::config::policy_snapshot_dir(),
            workspace: agent_core::config::policy_workspace_dir(),
            self_executable,
        };
        std::fs::create_dir_all(&provider.snapshot_dir)
            .map_err(|error| PolicyError::Storage(error.to_string()))?;
        std::fs::create_dir_all(&provider.workspace)
            .map_err(|error| PolicyError::Storage(error.to_string()))?;
        Ok(provider)
    }

    /// Продуктовые ограничения → списки исполняемых файлов для AppLocker.
    fn denied_executables(compiled: &CompiledPolicy) -> Vec<String> {
        let mut denied = Vec::new();
        if compiled.restrictions.cmd {
            denied.extend(CMD_EXECUTABLES.iter().map(|value| (*value).to_owned()));
        }
        if compiled.restrictions.powershell {
            denied.extend(
                POWERSHELL_EXECUTABLES
                    .iter()
                    .map(|value| (*value).to_owned()),
            );
        }
        denied
    }

    fn policy_xml(compiled: &CompiledPolicy) -> String {
        let allowed: Vec<String> = compiled.allowed_executables.iter().cloned().collect();
        let denied = Self::denied_executables(compiled);
        applocker::build_policy_xml(&allowed, &denied)
    }

    fn write_xml(&self, name: &str, xml: &str) -> Result<PathBuf, PolicyError> {
        let path = self.workspace.join(name);
        std::fs::write(&path, xml).map_err(|error| PolicyError::Storage(error.to_string()))?;
        Ok(path)
    }

    /// Полный перечень значений реестра, которыми управляет провайдер.
    /// Snapshot и apply обязаны ходить по одному и тому же списку.
    fn managed_values() -> Vec<(String, String)> {
        let mut values = vec![
            (EXPLORER_POLICIES.to_owned(), NO_CONTROL_PANEL.to_owned()),
            (STORE_POLICIES.to_owned(), REMOVE_WINDOWS_STORE.to_owned()),
            (
                ACTIVE_DESKTOP_POLICIES.to_owned(),
                NO_CHANGING_WALLPAPER.to_owned(),
            ),
        ];
        for (allowlist, blocklist) in BROWSER_POLICY_KEYS {
            for index in 1..=MAX_URL_RULES {
                values.push((allowlist.to_string(), index.to_string()));
                values.push((blocklist.to_string(), index.to_string()));
            }
        }
        values
    }

    fn apply_registry(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError> {
        let flag = |enabled: bool| RegistryData::Dword(u32::from(enabled));
        for (subkey, name, enabled) in [
            (
                EXPLORER_POLICIES,
                NO_CONTROL_PANEL,
                compiled.restrictions.settings,
            ),
            (
                STORE_POLICIES,
                REMOVE_WINDOWS_STORE,
                compiled.restrictions.store,
            ),
            (
                ACTIVE_DESKTOP_POLICIES,
                NO_CHANGING_WALLPAPER,
                compiled.restrictions.personalization,
            ),
        ] {
            if enabled {
                registry::write_value(subkey, name, &flag(true))
                    .map_err(|error| PolicyError::Provider(error.to_string()))?;
            } else {
                registry::delete_value(subkey, name)
                    .map_err(|error| PolicyError::Provider(error.to_string()))?;
            }
        }
        self.apply_browser_allowlist(compiled)
    }

    fn apply_browser_allowlist(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError> {
        let urls: Vec<&String> = compiled.allowed_urls.iter().take(MAX_URL_RULES).collect();
        for (allowlist, blocklist) in BROWSER_POLICY_KEYS {
            for index in 1..=MAX_URL_RULES {
                let name = index.to_string();
                match urls.get(index - 1) {
                    Some(url) => {
                        registry::write_value(allowlist, &name, &RegistryData::Text((*url).clone()))
                            .map_err(|error| PolicyError::Provider(error.to_string()))?
                    }
                    None => registry::delete_value(allowlist, &name)
                        .map_err(|error| PolicyError::Provider(error.to_string()))?,
                }
                // Блокировка "всё остальное" имеет смысл только вместе с
                // непустым allowlist, иначе браузер станет бесполезен.
                let blocked = index == 1 && !urls.is_empty();
                if blocked {
                    registry::write_value(blocklist, &name, &RegistryData::Text("*".to_owned()))
                        .map_err(|error| PolicyError::Provider(error.to_string()))?;
                } else {
                    registry::delete_value(blocklist, &name)
                        .map_err(|error| PolicyError::Provider(error.to_string()))?;
                }
            }
        }
        Ok(())
    }
}

impl PolicyProvider for WindowsPolicyProvider {
    type Snapshot = WindowsPolicySnapshot;

    fn check_support(&self) -> Capability {
        match applocker::application_identity_running() {
            Ok(true) => Capability::Enforced,
            Ok(false) => Capability::Unavailable(
                "служба Application Identity (AppIDSvc) не запущена, AppLocker не применяется"
                    .to_owned(),
            ),
            Err(error) => Capability::Unavailable(format!("AppLocker недоступен: {error}")),
        }
    }

    fn validate(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError> {
        // Обязательная проверка §6: политика проверяется до применения, и
        // именно на собственных бинарниках ClassOS.
        let candidate = self.write_xml("candidate.xml", &Self::policy_xml(compiled))?;
        match applocker::test_policy_file(&candidate, &self.self_executable)
            .map_err(|error| PolicyError::Provider(error.to_string()))?
        {
            PolicyDecision::Allowed => Ok(()),
            other => Err(PolicyError::Invalid(format!(
                "политика заблокировала бы сам ClassOS ({other:?})"
            ))),
        }
    }

    fn snapshot(&self) -> Result<Self::Snapshot, PolicyError> {
        let applocker_xml = applocker::current_policy_xml()
            .map_err(|error| PolicyError::Provider(error.to_string()))?;
        let mut backups = Vec::new();
        for (subkey, name) in Self::managed_values() {
            let value = registry::read_value(&subkey, &name)
                .map_err(|error| PolicyError::Provider(error.to_string()))?;
            backups.push(RegistryBackup {
                subkey,
                name,
                value: value.as_ref().map(StoredValue::from),
            });
        }
        Ok(WindowsPolicySnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            applocker_xml,
            registry: backups,
        })
    }

    fn apply(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError> {
        let path = self.write_xml("effective.xml", &Self::policy_xml(compiled))?;
        applocker::set_policy_from_file(&path)
            .map_err(|error| PolicyError::Provider(error.to_string()))?;
        self.apply_registry(compiled)
    }

    fn verify(&self, _compiled: &CompiledPolicy) -> Result<(), PolicyError> {
        match applocker::test_effective_policy(&self.self_executable)
            .map_err(|error| PolicyError::Provider(error.to_string()))?
        {
            PolicyDecision::Allowed => Ok(()),
            other => Err(PolicyError::Provider(format!(
                "после применения политики ClassOS оказался заблокирован ({other:?})"
            ))),
        }
    }

    fn rollback(&self, snapshot: &Self::Snapshot) -> Result<(), PolicyError> {
        let xml = if snapshot.applocker_xml.trim().is_empty() {
            applocker::empty_policy_xml()
        } else {
            snapshot.applocker_xml.clone()
        };
        let path = self.write_xml("rollback.xml", &xml)?;
        applocker::set_policy_from_file(&path)
            .map_err(|error| PolicyError::Provider(error.to_string()))?;
        // Реестр восстанавливается целиком, даже если часть значений не
        // менялась: так откат не зависит от того, на каком шаге был сбой.
        for backup in &snapshot.registry {
            registry::restore_value(
                &backup.subkey,
                &backup.name,
                backup.value.as_ref().map(RegistryData::from).as_ref(),
            )
            .map_err(|error| PolicyError::Provider(error.to_string()))?;
        }
        Ok(())
    }
}

impl SnapshotStore for WindowsPolicyProvider {
    fn store_snapshot(&self, snapshot: &Self::Snapshot) -> Result<String, PolicyError> {
        let path = self.snapshot_path(&snapshot.id);
        let text = serde_json::to_string_pretty(snapshot)
            .map_err(|error| PolicyError::Storage(error.to_string()))?;
        std::fs::write(&path, text).map_err(|error| PolicyError::Storage(error.to_string()))?;
        Ok(snapshot.id.clone())
    }

    fn load_snapshot(&self, snapshot_id: &str) -> Result<Self::Snapshot, PolicyError> {
        let path = self.snapshot_path(snapshot_id);
        let text = std::fs::read_to_string(&path).map_err(|error| {
            PolicyError::Storage(format!("снимок {snapshot_id} недоступен: {error}"))
        })?;
        serde_json::from_str(&text).map_err(|error| PolicyError::Storage(error.to_string()))
    }

    fn apply_verified(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError> {
        self.validate(compiled)?;
        self.apply(compiled)?;
        self.verify(compiled)
    }
}

impl WindowsPolicyProvider {
    fn snapshot_path(&self, snapshot_id: &str) -> PathBuf {
        // Идентификатор — UUID, сгенерированный нами; посторонний путь сюда
        // попасть не может, но имя всё равно приводится к файлу без каталогов.
        let safe: String = snapshot_id
            .chars()
            .filter(|value| value.is_ascii_alphanumeric() || *value == '-')
            .collect();
        self.snapshot_dir.join(format!("{safe}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn compiled(cmd: bool, powershell: bool, urls: &[&str]) -> CompiledPolicy {
        CompiledPolicy {
            policy_name: "Test".to_owned(),
            allowed_executables: BTreeSet::from(["Code.exe".to_owned()]),
            allowed_urls: urls.iter().map(|value| (*value).to_owned()).collect(),
            restrictions: policy_engine::Restrictions {
                cmd,
                powershell,
                ..Default::default()
            },
        }
    }

    #[test]
    fn restrictions_map_to_concrete_executables() {
        assert!(WindowsPolicyProvider::denied_executables(&compiled(false, false, &[])).is_empty());
        let denied = WindowsPolicyProvider::denied_executables(&compiled(true, true, &[]));
        assert!(denied.contains(&"cmd.exe".to_owned()));
        assert!(denied.contains(&"pwsh.exe".to_owned()));
    }

    #[test]
    fn generated_xml_allows_classos_and_denies_restricted_shells() {
        let mut policy = compiled(true, false, &[]);
        policy
            .allowed_executables
            .insert("classos-service.exe".to_owned());
        let xml = WindowsPolicyProvider::policy_xml(&policy);
        assert!(xml.contains(r"*\classos-service.exe"));
        assert!(xml.contains(r"*\cmd.exe"));
    }

    #[test]
    fn managed_values_cover_every_key_the_provider_writes() {
        let values = WindowsPolicyProvider::managed_values();
        // Иначе откат оставил бы часть ключей применёнными.
        assert!(
            values
                .iter()
                .any(|(key, name)| key == EXPLORER_POLICIES && name == NO_CONTROL_PANEL)
        );
        assert_eq!(
            values.len(),
            3 + BROWSER_POLICY_KEYS.len() * MAX_URL_RULES * 2
        );
    }
}
