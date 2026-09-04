//! Асинхронная обёртка над `PolicyService` для сетевого цикла Service.
//!
//! Применение политики ходит в PowerShell и реестр и занимает секунды, поэтому
//! выполняется на blocking-пуле: иначе один Apply останавливал бы heartbeat,
//! стриминг экрана и remote control для всего класса.

use std::sync::{Arc, Mutex};

use agent_core::policy::{PolicyOutcome, PolicyService};
use policy_engine::PolicyError;

use crate::policy_provider::WindowsPolicyProvider;

type Guarded = Arc<Mutex<PolicyService<WindowsPolicyProvider>>>;

#[derive(Clone)]
pub struct PolicyController {
    service: Guarded,
}

impl PolicyController {
    /// Загружает состояние политики устройства при старте Service.
    pub fn load() -> Result<Self, PolicyError> {
        let provider = WindowsPolicyProvider::new()?;
        let service = PolicyService::load(provider, &agent_core::config::policy_state_path())?;
        Ok(Self {
            service: Arc::new(Mutex::new(service)),
        })
    }

    async fn run<F>(&self, operation: F) -> Result<PolicyOutcome, PolicyError>
    where
        F: FnOnce(&mut PolicyService<WindowsPolicyProvider>) -> Result<PolicyOutcome, PolicyError>
            + Send
            + 'static,
    {
        let service = Arc::clone(&self.service);
        tokio::task::spawn_blocking(move || {
            let mut guard = service
                .lock()
                .map_err(|_| PolicyError::Provider("policy mutex poisoned".to_owned()))?;
            operation(&mut guard)
        })
        .await
        .map_err(|error| PolicyError::Provider(format!("policy task: {error}")))?
    }

    pub async fn apply_lesson(&self, document: Vec<u8>) -> Result<PolicyOutcome, PolicyError> {
        self.run(move |service| service.apply_lesson(&document))
            .await
    }

    pub async fn enable_focus(
        &self,
        allowed_application_ids: Vec<String>,
    ) -> Result<PolicyOutcome, PolicyError> {
        self.run(move |service| service.enable_focus(allowed_application_ids))
            .await
    }

    pub async fn disable_focus(&self) -> Result<PolicyOutcome, PolicyError> {
        self.run(|service| service.disable_focus()).await
    }

    pub async fn rollback(&self, snapshot_id: String) -> Result<PolicyOutcome, PolicyError> {
        self.run(move |service| service.rollback(&snapshot_id))
            .await
    }
}

/// Локальный break-glass: снимает активную политику без сети и без Teacher
/// Console (spec T6 §9). Вызывается из CLI, а не из сетевого обработчика —
/// сетевого маршрута к этой функции нет и быть не должно.
pub fn break_glass_locally() -> Result<(), PolicyError> {
    let provider = WindowsPolicyProvider::new()?;
    let mut service = PolicyService::load(provider, &agent_core::config::policy_state_path())?;
    service.break_glass().map(|_| ())
}
