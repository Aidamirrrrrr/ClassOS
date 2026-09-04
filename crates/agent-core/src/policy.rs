//! Применение политик на устройстве: слои, состояние и обработка команд T6.
//!
//! Модуль не знает ни AppLocker, ни registry — вся Windows-специфика скрыта за
//! `policy_engine::PolicyProvider` (ADR-0006). Благодаря этому логика
//! применения, Focus Mode, rollback и break-glass тестируются на любом хосте.

use std::path::{Path, PathBuf};

use policy_engine::{
    ApplicationCatalog, CompiledPolicy, LessonPolicy, PersistedPolicyState, PolicyDocument,
    PolicyError, PolicyProvider, apply_safely, compile,
};

use crate::commands::catalog_application;

/// Каталог приложений устройства: тот же, что используют classroom-команды T5.
/// Teacher Console присылает продуктовый идентификатор, устройство само решает,
/// какому исполняемому файлу он соответствует.
pub struct DeviceCatalog;

impl ApplicationCatalog for DeviceCatalog {
    fn resolve(&self, application_id: &str) -> Option<String> {
        catalog_application(application_id).map(|value| value.executable().to_owned())
    }
}

/// Результат применения политики для протокола.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyOutcome {
    pub policy_name: String,
    pub snapshot_id: Option<String>,
    pub focus_active: bool,
}

/// Хранилище состояния и владелец провайдера enforcement.
///
/// Все переходы проходят через один путь `apply_effective`, поэтому нельзя
/// применить политику, минуя компиляцию с обязательным auto-allow.
pub struct PolicyService<P: PolicyProvider> {
    provider: P,
    state_path: PathBuf,
    state: PersistedPolicyState,
}

impl<P: SnapshotStore> PolicyService<P> {
    /// Загружает сохранённое состояние. Отсутствие файла — нормальный первый
    /// запуск, а не ошибка.
    pub fn load(provider: P, state_path: &Path) -> Result<Self, PolicyError> {
        let state = PersistedPolicyState::load(state_path)?;
        Ok(Self {
            provider,
            state_path: state_path.to_owned(),
            state,
        })
    }

    pub fn layers(&self) -> &policy_engine::PolicyLayers {
        &self.state.layers
    }

    pub fn focus_is_active(&self) -> bool {
        self.state.layers.focus_is_active()
    }

    pub fn active_snapshot_id(&self) -> Option<&str> {
        self.state.active_snapshot_id.as_deref()
    }

    /// Применяет lesson-политику, присланную Teacher Console.
    pub fn apply_lesson(&mut self, document: &[u8]) -> Result<PolicyOutcome, PolicyError> {
        let document = PolicyDocument::decode(document)?;
        // Компиляция до изменения состояния: некорректная политика не должна
        // оставлять слой изменённым.
        self.compiled_with_lesson(&document.policy)?;
        let previous = std::mem::replace(&mut self.state.layers.lesson, document.policy);
        match self.apply_effective() {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.state.layers.lesson = previous;
                Err(error)
            }
        }
    }

    /// Включает Focus Mode как временный слой поверх действующей политики.
    pub fn enable_focus(
        &mut self,
        allowed_application_ids: Vec<String>,
    ) -> Result<PolicyOutcome, PolicyError> {
        if allowed_application_ids.is_empty() {
            return Err(PolicyError::Invalid(
                "Focus Mode требует хотя бы одно разрешённое приложение".to_owned(),
            ));
        }
        let previous = self.state.layers.temporary_override.clone();
        self.state.layers.enable_focus(allowed_application_ids);
        match self.apply_effective() {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.state.layers.temporary_override = previous;
                Err(error)
            }
        }
    }

    /// Выключает Focus Mode и пересчитывает EffectivePolicy из оставшихся слоёв.
    pub fn disable_focus(&mut self) -> Result<PolicyOutcome, PolicyError> {
        let previous = self.state.layers.temporary_override.clone();
        self.state.layers.disable_focus();
        match self.apply_effective() {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.state.layers.temporary_override = previous;
                Err(error)
            }
        }
    }

    /// Явный откат к состоянию устройства до активной политики.
    pub fn rollback(&mut self, snapshot_id: &str) -> Result<PolicyOutcome, PolicyError> {
        let active = self.state.active_snapshot_id.as_deref().ok_or_else(|| {
            PolicyError::Invalid("на устройстве нет активной политики".to_owned())
        })?;
        if !snapshot_id.is_empty() && snapshot_id != active {
            return Err(PolicyError::Invalid(format!(
                "снимок {snapshot_id} не является активным на устройстве"
            )));
        }
        self.restore_baseline(active.to_owned())
    }

    /// Break-glass: снимает политику локально, без сети и без Teacher Console.
    pub fn break_glass(&mut self) -> Result<PolicyOutcome, PolicyError> {
        let Some(active) = self.state.active_snapshot_id.clone() else {
            return Ok(PolicyOutcome {
                policy_name: String::new(),
                snapshot_id: None,
                focus_active: false,
            });
        };
        self.restore_baseline(active)
    }

    fn restore_baseline(&mut self, snapshot_id: String) -> Result<PolicyOutcome, PolicyError> {
        let snapshot = self.provider.load_snapshot(&snapshot_id)?;
        self.provider.rollback(&snapshot)?;
        // Слои очищаются только после успешного отката: иначе состояние на
        // диске разошлось бы с реальным состоянием устройства.
        self.state.layers.lesson = LessonPolicy::default();
        self.state.layers.temporary_override = None;
        self.state.active_snapshot_id = None;
        self.state.save(&self.state_path)?;
        Ok(PolicyOutcome {
            policy_name: String::new(),
            snapshot_id: None,
            focus_active: false,
        })
    }

    fn compiled_with_lesson(&self, lesson: &LessonPolicy) -> Result<CompiledPolicy, PolicyError> {
        let mut layers = self.state.layers.clone();
        layers.lesson = lesson.clone();
        compile(&layers.effective(), &DeviceCatalog)
    }

    /// Единственный путь применения: компиляция → безопасный rollout →
    /// сохранение состояния.
    fn apply_effective(&mut self) -> Result<PolicyOutcome, PolicyError> {
        let effective = self.state.layers.effective();
        let compiled = compile(&effective, &DeviceCatalog)?;
        // Первый снимок сохраняется как baseline: повторное применение не
        // должно затирать исходное состояние устройства снимком уже
        // применённой политики.
        let snapshot_id = match self.state.active_snapshot_id.clone() {
            Some(existing) => {
                if let Err(error) = self.provider.apply_verified(&compiled) {
                    // Инвариант IV: после первого изменения устройство не
                    // остаётся в частично применённом состоянии. Возврат идёт к
                    // baseline — единственному состоянию, о котором у нас есть
                    // достоверный снимок.
                    self.restore_baseline(existing)?;
                    return Err(error);
                }
                existing
            }
            None => {
                let snapshot = apply_safely(&self.provider, &compiled)?;
                self.provider.store_snapshot(&snapshot)?
            }
        };
        self.state.active_snapshot_id = Some(snapshot_id.clone());
        self.state.save(&self.state_path)?;
        Ok(PolicyOutcome {
            policy_name: compiled.policy_name,
            snapshot_id: Some(snapshot_id),
            focus_active: self.state.layers.focus_is_active(),
        })
    }
}

/// Провайдер, умеющий сохранять снимки между перезагрузками.
///
/// Требование ADR-0014: rollback обязан пережить перезагрузку и падение
/// Service, иначе устройство останется заблокированным без пути назад.
pub trait SnapshotStore: PolicyProvider {
    fn store_snapshot(&self, snapshot: &Self::Snapshot) -> Result<String, PolicyError>;
    fn load_snapshot(&self, snapshot_id: &str) -> Result<Self::Snapshot, PolicyError>;
    /// Повторное применение поверх уже активной политики без нового снимка.
    fn apply_verified(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_engine::Capability;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeProvider {
        capability: Option<String>,
        verify_fails: RefCell<bool>,
        snapshots: RefCell<HashMap<String, String>>,
        /// Состояние "устройства": что сейчас реально применено.
        enforced: RefCell<Option<CompiledPolicy>>,
        applies: RefCell<usize>,
    }

    impl PolicyProvider for FakeProvider {
        type Snapshot = String;

        fn check_support(&self) -> Capability {
            match &self.capability {
                Some(reason) => Capability::Unavailable(reason.clone()),
                None => Capability::Enforced,
            }
        }
        fn validate(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError> {
            compiled
                .allowed_executables
                .contains("classos-service.exe")
                .then_some(())
                .ok_or(PolicyError::Invalid("нет allow для ClassOS".to_owned()))
        }
        fn snapshot(&self) -> Result<String, PolicyError> {
            Ok(match self.enforced.borrow().as_ref() {
                Some(policy) => policy.policy_name.clone(),
                None => "baseline".to_owned(),
            })
        }
        fn apply(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError> {
            *self.applies.borrow_mut() += 1;
            *self.enforced.borrow_mut() = Some(compiled.clone());
            Ok(())
        }
        fn verify(&self, _: &CompiledPolicy) -> Result<(), PolicyError> {
            if *self.verify_fails.borrow() {
                return Err(PolicyError::Provider("verify failed".to_owned()));
            }
            Ok(())
        }
        fn rollback(&self, snapshot: &String) -> Result<(), PolicyError> {
            *self.enforced.borrow_mut() = None;
            assert_eq!(
                snapshot, "baseline",
                "откат должен вести к исходному состоянию"
            );
            Ok(())
        }
    }

    impl SnapshotStore for FakeProvider {
        fn store_snapshot(&self, snapshot: &String) -> Result<String, PolicyError> {
            let id = format!("snapshot-{}", self.snapshots.borrow().len() + 1);
            self.snapshots
                .borrow_mut()
                .insert(id.clone(), snapshot.clone());
            Ok(id)
        }
        fn load_snapshot(&self, snapshot_id: &str) -> Result<String, PolicyError> {
            self.snapshots
                .borrow()
                .get(snapshot_id)
                .cloned()
                .ok_or_else(|| PolicyError::Storage(format!("нет снимка {snapshot_id}")))
        }
        fn apply_verified(&self, compiled: &CompiledPolicy) -> Result<(), PolicyError> {
            self.validate(compiled)?;
            self.apply(compiled)?;
            self.verify(compiled)
        }
    }

    struct TempState {
        directory: PathBuf,
    }

    impl TempState {
        fn new(tag: &str) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "classos-policy-service-{}-{tag}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&directory);
            Self { directory }
        }
        fn path(&self) -> PathBuf {
            self.directory.join("policy.toml")
        }
    }

    impl Drop for TempState {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    fn document(name: &str, applications: &[&str]) -> Vec<u8> {
        PolicyDocument::new(LessonPolicy {
            name: name.to_owned(),
            allowed_applications: applications.iter().map(|v| (*v).to_owned()).collect(),
            block_cmd: true,
            ..Default::default()
        })
        .encode()
        .unwrap()
    }

    fn service(temp: &TempState) -> PolicyService<FakeProvider> {
        PolicyService::load(FakeProvider::default(), &temp.path()).unwrap()
    }

    #[test]
    fn apply_lesson_reaches_device_and_persists_state() {
        let temp = TempState::new("apply");
        let mut service = service(&temp);
        let outcome = service
            .apply_lesson(&document("Python", &["python", "vscode"]))
            .unwrap();

        assert_eq!(outcome.policy_name, "Python");
        assert!(outcome.snapshot_id.is_some());
        let enforced = service.provider.enforced.borrow().clone().unwrap();
        assert!(enforced.allowed_executables.contains("python.exe"));
        assert!(enforced.allowed_executables.contains("classos-service.exe"));
        assert!(enforced.restrictions.cmd);

        // Состояние переживает перезапуск Service.
        let reloaded = PersistedPolicyState::load(&temp.path()).unwrap();
        assert_eq!(reloaded.layers.lesson.name, "Python");
        assert!(reloaded.active_snapshot_id.is_some());
    }

    #[test]
    fn invalid_policy_is_rejected_before_touching_device() {
        let temp = TempState::new("invalid");
        let mut service = service(&temp);
        let error = service
            .apply_lesson(&document("Bad", &["telegram"]))
            .unwrap_err();

        assert_eq!(error.code(), "POLICY_INVALID");
        assert!(service.provider.enforced.borrow().is_none());
        assert_eq!(*service.provider.applies.borrow(), 0);
        assert!(service.layers().lesson.name.is_empty());
    }

    #[test]
    fn verify_failure_rolls_back_and_keeps_previous_layers() {
        let temp = TempState::new("verify");
        let mut service = service(&temp);
        *service.provider.verify_fails.borrow_mut() = true;

        let error = service
            .apply_lesson(&document("Python", &["python"]))
            .unwrap_err();

        assert_eq!(error.code(), "POLICY_APPLY_FAILED");
        // Устройство вернулось в исходное состояние, слой не сохранён.
        assert!(service.provider.enforced.borrow().is_none());
        assert!(service.layers().lesson.name.is_empty());
        assert!(service.active_snapshot_id().is_none());
    }

    #[test]
    fn unsupported_device_reports_honestly_and_changes_nothing() {
        let temp = TempState::new("unsupported");
        let provider = FakeProvider {
            capability: Some("служба AppIDSvc не запущена".to_owned()),
            ..FakeProvider::default()
        };
        let mut service = PolicyService::load(provider, &temp.path()).unwrap();

        let error = service
            .apply_lesson(&document("Python", &["python"]))
            .unwrap_err();

        assert_eq!(error.code(), "POLICY_UNSUPPORTED");
        assert!(service.provider.enforced.borrow().is_none());
        assert!(service.layers().lesson.name.is_empty());
    }

    #[test]
    fn focus_cycles_return_to_lesson_policy_every_time() {
        let temp = TempState::new("focus");
        let mut service = service(&temp);
        service
            .apply_lesson(&document("Python", &["python", "chrome"]))
            .unwrap();

        for _ in 0..3 {
            service.enable_focus(vec!["vscode".to_owned()]).unwrap();
            let enforced = service.provider.enforced.borrow().clone().unwrap();
            assert!(enforced.allowed_executables.contains("Code.exe"));
            assert!(!enforced.allowed_executables.contains("chrome.exe"));
            assert!(service.focus_is_active());

            service.disable_focus().unwrap();
            let enforced = service.provider.enforced.borrow().clone().unwrap();
            assert!(enforced.allowed_executables.contains("chrome.exe"));
            assert!(!enforced.allowed_executables.contains("Code.exe"));
            assert!(!service.focus_is_active());
        }
    }

    #[test]
    fn focus_without_applications_is_rejected() {
        let temp = TempState::new("focus-empty");
        let mut service = service(&temp);
        assert!(service.enable_focus(Vec::new()).is_err());
    }

    #[test]
    fn break_glass_restores_baseline_without_network() {
        let temp = TempState::new("break-glass");
        let mut service = service(&temp);
        service
            .apply_lesson(&document("Python", &["python"]))
            .unwrap();
        service.enable_focus(vec!["vscode".to_owned()]).unwrap();

        service.break_glass().unwrap();

        assert!(service.provider.enforced.borrow().is_none());
        assert!(service.active_snapshot_id().is_none());
        assert!(!service.focus_is_active());
        // Повторный break-glass на чистом устройстве безопасен.
        assert!(service.break_glass().is_ok());
    }

    #[test]
    fn rollback_rejects_foreign_snapshot_id() {
        let temp = TempState::new("rollback");
        let mut service = service(&temp);
        service
            .apply_lesson(&document("Python", &["python"]))
            .unwrap();

        assert!(service.rollback("snapshot-999").is_err());
        // Активная политика осталась нетронутой.
        assert!(service.provider.enforced.borrow().is_some());

        let active = service.active_snapshot_id().unwrap().to_owned();
        service.rollback(&active).unwrap();
        assert!(service.provider.enforced.borrow().is_none());
    }

    #[test]
    fn failure_on_second_apply_returns_device_to_baseline() {
        let temp = TempState::new("second-apply");
        let mut service = service(&temp);
        service
            .apply_lesson(&document("Python", &["python"]))
            .unwrap();

        *service.provider.verify_fails.borrow_mut() = true;
        let error = service
            .apply_lesson(&document("Roblox", &["chrome"]))
            .unwrap_err();

        assert_eq!(error.code(), "POLICY_APPLY_FAILED");
        // Частично применённое состояние недопустимо: устройство возвращается
        // к состоянию до ClassOS, а не остаётся с половиной новой политики.
        assert!(service.provider.enforced.borrow().is_none());
        assert!(service.active_snapshot_id().is_none());
    }

    #[test]
    fn repeated_apply_keeps_original_baseline_snapshot() {
        let temp = TempState::new("baseline");
        let mut service = service(&temp);
        let first = service
            .apply_lesson(&document("Python", &["python"]))
            .unwrap();
        let second = service
            .apply_lesson(&document("Roblox", &["chrome"]))
            .unwrap();

        // Иначе откат вернул бы устройство к первой политике, а не к
        // состоянию до ClassOS.
        assert_eq!(first.snapshot_id, second.snapshot_id);
        service.break_glass().unwrap();
        assert!(service.provider.enforced.borrow().is_none());
    }
}
