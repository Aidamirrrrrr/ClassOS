//! Каталог приложений, software-профили и расчёт drift.
//!
//! Крейт переносим: работа с WinGet скрыта за трейтом [`PackageManager`].
//!
//! Каталог здесь — **единственный** в системе. `LaunchApplication` из T5,
//! компилятор политик T6 и software management T7 берут определения отсюда, а
//! не заводят параллельные списки (spec T7 §5).

use std::collections::BTreeMap;

/// Продуктовое определение приложения.
///
/// Приложение опознаётся определением, а не строкой `processName == "Code.exe"`
/// (`01_TECHNICAL_ARCHITECTURE.md` §58).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    /// Имена исполняемых файлов; первое считается основным.
    pub executables: &'static [&'static str],
    pub publisher: &'static str,
    /// Идентификатор пакета в Windows Package Manager.
    pub winget_id: &'static str,
    /// Версия, одобренная школой. Обновление Python или Unity посреди
    /// программы ломает учебный курс, поэтому `latest` здесь не значение по
    /// умолчанию, а осознанный выбор (`01_ROADMAP.md` §37).
    pub approved_version: ApprovedVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovedVersion {
    /// Конкретная одобренная версия.
    Pinned(&'static str),
    /// Осознанно разрешено ставить последнюю доступную версию.
    Latest,
}

impl ApplicationDefinition {
    pub fn primary_executable(&self) -> &'static str {
        self.executables.first().copied().unwrap_or_default()
    }
}

/// Approved catalog. Всё, чего здесь нет, установить нельзя: иначе Teacher
/// Console превращается в систему удалённого выполнения кода (spec T7 §6.2).
pub const CATALOG: &[ApplicationDefinition] = &[
    ApplicationDefinition {
        id: "vscode",
        display_name: "Visual Studio Code",
        executables: &["Code.exe"],
        publisher: "Microsoft Corporation",
        winget_id: "Microsoft.VisualStudioCode",
        approved_version: ApprovedVersion::Latest,
    },
    ApplicationDefinition {
        id: "python",
        display_name: "Python",
        executables: &["python.exe"],
        publisher: "Python Software Foundation",
        winget_id: "Python.Python.3.13",
        approved_version: ApprovedVersion::Pinned("3.13"),
    },
    ApplicationDefinition {
        id: "chrome",
        display_name: "Google Chrome",
        executables: &["chrome.exe"],
        publisher: "Google LLC",
        winget_id: "Google.Chrome",
        approved_version: ApprovedVersion::Latest,
    },
    ApplicationDefinition {
        id: "git",
        display_name: "Git",
        executables: &["git.exe"],
        publisher: "The Git Development Community",
        winget_id: "Git.Git",
        approved_version: ApprovedVersion::Latest,
    },
];

/// Ищет определение по продуктовому идентификатору.
pub fn definition(application_id: &str) -> Option<&'static ApplicationDefinition> {
    CATALOG.iter().find(|value| value.id == application_id)
}

// ---------------------------------------------------------------------------
// Профили и drift
// ---------------------------------------------------------------------------

/// Требование профиля к одному приложению.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub application_id: &'static str,
    /// Требуемый префикс версии (`"3.13"`). `None` — достаточно наличия.
    pub required_version: Option<&'static str>,
}

/// Software Profile кабинета: желаемое состояние устройства.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftwareProfile {
    pub id: &'static str,
    pub name: &'static str,
    pub requirements: &'static [Requirement],
}

pub const PROFILES: &[SoftwareProfile] = &[
    SoftwareProfile {
        id: "python-classroom",
        name: "Python Classroom",
        requirements: &[
            Requirement {
                application_id: "python",
                required_version: Some("3.13"),
            },
            Requirement {
                application_id: "vscode",
                required_version: None,
            },
            Requirement {
                application_id: "git",
                required_version: None,
            },
            Requirement {
                application_id: "chrome",
                required_version: None,
            },
        ],
    },
    SoftwareProfile {
        id: "web-classroom",
        name: "Web Classroom",
        requirements: &[
            Requirement {
                application_id: "vscode",
                required_version: None,
            },
            Requirement {
                application_id: "chrome",
                required_version: None,
            },
        ],
    },
];

pub fn profile(profile_id: &str) -> Option<&'static SoftwareProfile> {
    PROFILES.iter().find(|value| value.id == profile_id)
}

/// Фактическое состояние: какие приложения и каких версий установлены.
pub type InstalledSoftware = BTreeMap<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftKind {
    Missing,
    VersionMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub application_id: String,
    pub kind: DriftKind,
    pub required_version: String,
    pub actual_version: String,
}

/// Считает расхождение желаемого и фактического состояния.
pub fn drift(profile: &SoftwareProfile, installed: &InstalledSoftware) -> Vec<Drift> {
    let mut result = Vec::new();
    for requirement in profile.requirements {
        let actual = installed.get(requirement.application_id);
        match (actual, requirement.required_version) {
            (None, _) => result.push(Drift {
                application_id: requirement.application_id.to_owned(),
                kind: DriftKind::Missing,
                required_version: requirement.required_version.unwrap_or_default().to_owned(),
                actual_version: String::new(),
            }),
            (Some(version), Some(required)) if !version_satisfies(version, required) => {
                result.push(Drift {
                    application_id: requirement.application_id.to_owned(),
                    kind: DriftKind::VersionMismatch,
                    required_version: required.to_owned(),
                    actual_version: version.clone(),
                });
            }
            _ => {}
        }
    }
    result
}

/// Требование задаётся префиксом версии: `3.13` принимает `3.13.2`, но не
/// `3.14.0` и не `3.1`.
fn version_satisfies(actual: &str, required: &str) -> bool {
    let mut actual_parts = actual.split('.');
    required
        .split('.')
        .all(|part| actual_parts.next() == Some(part))
}

// ---------------------------------------------------------------------------
// Операции над пакетами
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SoftwareError {
    #[error("приложение отсутствует в approved catalog: {0}")]
    NotApproved(String),
    #[error("профиль отсутствует: {0}")]
    UnknownProfile(String),
    #[error("менеджер пакетов: {0}")]
    PackageManager(String),
}

impl SoftwareError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotApproved(_) => "PACKAGE_NOT_APPROVED",
            Self::UnknownProfile(_) => "PROFILE_UNKNOWN",
            Self::PackageManager(_) => "PACKAGE_OPERATION_FAILED",
        }
    }
}

/// Менеджер пакетов устройства.
///
/// Принимает только `ApplicationDefinition` из каталога — произвольный
/// package query в этот трейт передать невозможно by design.
pub trait PackageManager {
    fn detect(&self, definition: &ApplicationDefinition) -> Result<Option<String>, SoftwareError>;
    fn install(&self, definition: &ApplicationDefinition) -> Result<(), SoftwareError>;
    fn repair(&self, definition: &ApplicationDefinition) -> Result<(), SoftwareError>;
}

/// Снимает фактическое состояние по всему каталогу.
pub fn inventory<M: PackageManager>(manager: &M) -> Result<InstalledSoftware, SoftwareError> {
    let mut installed = InstalledSoftware::new();
    for definition in CATALOG {
        if let Some(version) = manager.detect(definition)? {
            installed.insert(definition.id.to_owned(), version);
        }
    }
    Ok(installed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairItem {
    pub application_id: String,
    pub success: bool,
    pub error_code: String,
}

/// Приводит устройство к desired state профиля.
///
/// Repair трогает только то, что перечислено в `profile_id`: «заодно»
/// исправлять что-то ещё запрещено (spec T7 §10.4).
pub fn repair_profile<M: PackageManager>(
    manager: &M,
    profile_id: &str,
    installed: &InstalledSoftware,
) -> Result<Vec<RepairItem>, SoftwareError> {
    let profile =
        profile(profile_id).ok_or_else(|| SoftwareError::UnknownProfile(profile_id.to_owned()))?;
    let mut items = Vec::new();
    for entry in drift(profile, installed) {
        let Some(definition) = definition(&entry.application_id) else {
            items.push(RepairItem {
                application_id: entry.application_id.clone(),
                success: false,
                error_code: "PACKAGE_NOT_APPROVED".to_owned(),
            });
            continue;
        };
        let outcome = match entry.kind {
            DriftKind::Missing => manager.install(definition),
            DriftKind::VersionMismatch => manager.repair(definition),
        };
        // Проверка результата, а не доверие коду возврата установщика:
        // «установилось» и «стало соответствовать профилю» — разные вещи.
        let verified = outcome.and_then(|()| manager.detect(definition));
        items.push(match verified {
            Ok(Some(version))
                if entry.required_version.is_empty()
                    || version_satisfies(&version, &entry.required_version) =>
            {
                RepairItem {
                    application_id: entry.application_id,
                    success: true,
                    error_code: String::new(),
                }
            }
            Ok(_) => RepairItem {
                application_id: entry.application_id,
                success: false,
                error_code: "REPAIR_VERIFY_FAILED".to_owned(),
            },
            Err(error) => RepairItem {
                application_id: entry.application_id,
                success: false,
                error_code: error.code().to_owned(),
            },
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn installed(pairs: &[(&str, &str)]) -> InstalledSoftware {
        pairs
            .iter()
            .map(|(id, version)| ((*id).to_owned(), (*version).to_owned()))
            .collect()
    }

    #[test]
    fn catalog_is_the_only_source_of_application_ids() {
        assert!(definition("vscode").is_some());
        // Произвольный пакет установить нельзя.
        assert!(definition("Telegram.TelegramDesktop").is_none());
        assert!(definition("python.exe").is_none());
    }

    #[test]
    fn pinned_version_is_not_latest_by_default() {
        assert_eq!(
            definition("python").unwrap().approved_version,
            ApprovedVersion::Pinned("3.13")
        );
    }

    #[test]
    fn version_prefix_matching_is_strict() {
        assert!(version_satisfies("3.13.2", "3.13"));
        assert!(version_satisfies("3.13", "3.13"));
        assert!(!version_satisfies("3.14.0", "3.13"));
        // "3.1" не должно проходить под требование "3.13".
        assert!(!version_satisfies("3.1", "3.13"));
    }

    #[test]
    fn drift_reports_missing_and_mismatched_packages() {
        let profile = profile("python-classroom").unwrap();
        let state = installed(&[("python", "3.12.1"), ("vscode", "1.95.0"), ("git", "2.47")]);
        let result = drift(profile, &state);

        assert_eq!(result.len(), 2);
        let python = result
            .iter()
            .find(|value| value.application_id == "python")
            .unwrap();
        assert_eq!(python.kind, DriftKind::VersionMismatch);
        assert_eq!(python.actual_version, "3.12.1");
        let chrome = result
            .iter()
            .find(|value| value.application_id == "chrome")
            .unwrap();
        assert_eq!(chrome.kind, DriftKind::Missing);
    }

    #[test]
    fn profile_in_desired_state_has_no_drift() {
        let profile = profile("web-classroom").unwrap();
        let state = installed(&[("vscode", "1.95.0"), ("chrome", "131.0")]);
        assert!(drift(profile, &state).is_empty());
    }

    #[derive(Default)]
    struct FakeManager {
        installed: RefCell<InstalledSoftware>,
        install_calls: RefCell<Vec<String>>,
        fail: Option<&'static str>,
        /// Установка «проходит», но версия остаётся прежней.
        silent_noop: bool,
    }

    impl PackageManager for FakeManager {
        fn detect(
            &self,
            definition: &ApplicationDefinition,
        ) -> Result<Option<String>, SoftwareError> {
            Ok(self.installed.borrow().get(definition.id).cloned())
        }
        fn install(&self, definition: &ApplicationDefinition) -> Result<(), SoftwareError> {
            self.install_calls
                .borrow_mut()
                .push(definition.id.to_owned());
            if self.fail == Some(definition.id) {
                return Err(SoftwareError::PackageManager("winget exit 1".to_owned()));
            }
            if !self.silent_noop {
                let version = match definition.approved_version {
                    ApprovedVersion::Pinned(value) => format!("{value}.0"),
                    ApprovedVersion::Latest => "1.0.0".to_owned(),
                };
                self.installed
                    .borrow_mut()
                    .insert(definition.id.to_owned(), version);
            }
            Ok(())
        }
        fn repair(&self, definition: &ApplicationDefinition) -> Result<(), SoftwareError> {
            self.install(definition)
        }
    }

    #[test]
    fn repair_installs_only_what_the_profile_requires() {
        let manager = FakeManager::default();
        let state = installed(&[("vscode", "1.95.0"), ("chrome", "131.0")]);
        let items = repair_profile(&manager, "web-classroom", &state).unwrap();

        assert!(items.is_empty());
        // Профиль уже в desired state: ничего ставить не нужно.
        assert!(manager.install_calls.borrow().is_empty());
    }

    #[test]
    fn repair_fixes_missing_and_mismatched_and_verifies_result() {
        let manager = FakeManager::default();
        let state = installed(&[("python", "3.12.1"), ("vscode", "1.95.0")]);
        let items = repair_profile(&manager, "python-classroom", &state).unwrap();

        assert_eq!(items.len(), 3);
        assert!(items.iter().all(|item| item.success), "{items:?}");
        let calls = manager.install_calls.borrow().clone();
        assert!(calls.contains(&"python".to_owned()));
        assert!(calls.contains(&"chrome".to_owned()));
        // git и chrome отсутствовали, vscode был в порядке и не трогался.
        assert!(!calls.contains(&"vscode".to_owned()));
    }

    #[test]
    fn repair_reports_partial_failure_without_aborting_the_rest() {
        let manager = FakeManager {
            fail: Some("chrome"),
            ..FakeManager::default()
        };
        let state = installed(&[("python", "3.13.1"), ("vscode", "1.95.0")]);
        let items = repair_profile(&manager, "python-classroom", &state).unwrap();

        let chrome = items
            .iter()
            .find(|item| item.application_id == "chrome")
            .unwrap();
        assert!(!chrome.success);
        assert_eq!(chrome.error_code, "PACKAGE_OPERATION_FAILED");
        let git = items
            .iter()
            .find(|item| item.application_id == "git")
            .unwrap();
        assert!(git.success, "сбой одного пакета не отменяет остальные");
    }

    #[test]
    fn silent_install_without_effect_is_reported_as_failure() {
        let manager = FakeManager {
            silent_noop: true,
            ..FakeManager::default()
        };
        let items = repair_profile(&manager, "web-classroom", &InstalledSoftware::new()).unwrap();

        assert!(items.iter().all(|item| !item.success));
        assert!(
            items
                .iter()
                .all(|item| item.error_code == "REPAIR_VERIFY_FAILED")
        );
    }

    #[test]
    fn unknown_profile_is_rejected() {
        let manager = FakeManager::default();
        let error = repair_profile(&manager, "minecraft", &InstalledSoftware::new()).unwrap_err();
        assert_eq!(error.code(), "PROFILE_UNKNOWN");
    }

    #[test]
    fn inventory_collects_versions_for_installed_catalog_entries() {
        let manager = FakeManager::default();
        manager
            .installed
            .borrow_mut()
            .insert("git".to_owned(), "2.47.0".to_owned());
        let state = inventory(&manager).unwrap();
        assert_eq!(state.get("git"), Some(&"2.47.0".to_owned()));
        assert_eq!(state.len(), 1);
    }
}
