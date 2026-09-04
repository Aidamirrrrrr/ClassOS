//! Сбор health-отчёта устройства и операции над пакетами (T7).
//!
//! Здесь Windows-примитивы из `windows-platform` соединяются с переносимыми
//! правилами `device-health` и каталогом `software-manager`. Правил и порогов
//! в этом файле нет — только сбор фактов.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use device_health::{AgentFacts, HealthAssessment, HealthError, HealthMetrics, MetricsProvider};
use software_manager::{
    ApplicationDefinition, InstalledSoftware, PackageManager, RepairItem, SoftwareError, drift,
    inventory, profile, repair_profile,
};
use windows_platform::{metrics, winget};

/// Инвентаризация опрашивает winget по каждому пакету и занимает секунды,
/// поэтому результат кэшируется. Health-отчёт уходит куда чаще.
const INVENTORY_TTL: Duration = Duration::from_secs(15 * 60);

pub struct WindowsMetrics {
    cpu: Mutex<metrics::CpuSampler>,
}

impl Default for WindowsMetrics {
    fn default() -> Self {
        Self {
            cpu: Mutex::new(metrics::CpuSampler::new()),
        }
    }
}

impl MetricsProvider for WindowsMetrics {
    fn collect(&self) -> Result<HealthMetrics, HealthError> {
        let cpu_percent = self
            .cpu
            .lock()
            .map_err(|_| HealthError::Collect("cpu sampler poisoned".to_owned()))?
            .sample()
            .map_err(|error| HealthError::Collect(error.to_string()))?;
        Ok(HealthMetrics {
            cpu_percent,
            ram_percent: metrics::memory_percent()
                .map_err(|error| HealthError::Collect(error.to_string()))?,
            disk_percent: metrics::system_disk_percent()
                .map_err(|error| HealthError::Collect(error.to_string()))?,
            uptime_seconds: metrics::uptime_seconds(),
            os_version: metrics::os_version()
                .map_err(|error| HealthError::Collect(error.to_string()))?,
        })
    }
}

/// Менеджер пакетов поверх winget.
///
/// Принимает только `ApplicationDefinition` из approved catalog: произвольный
/// package id в эту реализацию попасть не может (spec T7 §6.2).
#[derive(Default)]
pub struct WingetManager;

impl PackageManager for WingetManager {
    fn detect(&self, definition: &ApplicationDefinition) -> Result<Option<String>, SoftwareError> {
        winget::installed_version(definition.winget_id)
            .map_err(|error| SoftwareError::PackageManager(error.to_string()))
    }

    fn install(&self, definition: &ApplicationDefinition) -> Result<(), SoftwareError> {
        winget::install(definition.winget_id, approved_version(definition))
            .map_err(|error| SoftwareError::PackageManager(error.to_string()))
    }

    fn repair(&self, definition: &ApplicationDefinition) -> Result<(), SoftwareError> {
        winget::upgrade(definition.winget_id, approved_version(definition))
            .map_err(|error| SoftwareError::PackageManager(error.to_string()))
    }
}

/// Одобренная версия передаётся в winget явно. `latest` — не значение по
/// умолчанию, а отдельно принятое решение по конкретному приложению
/// (`01_ROADMAP.md` §37).
fn approved_version(definition: &ApplicationDefinition) -> Option<&'static str> {
    match definition.approved_version {
        software_manager::ApprovedVersion::Pinned(value) => Some(value),
        software_manager::ApprovedVersion::Latest => None,
    }
}

/// Кэш фактического состава ПО.
struct InventoryCache {
    software: InstalledSoftware,
    refreshed_at: Option<Instant>,
}

pub struct HealthCollector {
    metrics: WindowsMetrics,
    packages: WingetManager,
    profile_id: String,
    inventory: Mutex<InventoryCache>,
}

impl HealthCollector {
    pub fn new(profile_id: String) -> Self {
        Self {
            metrics: WindowsMetrics::default(),
            packages: WingetManager,
            profile_id,
            inventory: Mutex::new(InventoryCache {
                software: InstalledSoftware::new(),
                refreshed_at: None,
            }),
        }
    }

    /// Возвращает состав ПО, обновляя кэш не чаще, чем раз в TTL.
    fn software(&self, force: bool) -> InstalledSoftware {
        let mut cache = match self.inventory.lock() {
            Ok(cache) => cache,
            Err(_) => return InstalledSoftware::new(),
        };
        let stale = cache
            .refreshed_at
            .is_none_or(|value| value.elapsed() >= INVENTORY_TTL);
        if force || stale {
            match inventory(&self.packages) {
                Ok(software) => {
                    cache.software = software;
                    cache.refreshed_at = Some(Instant::now());
                }
                Err(error) => {
                    // Устаревший инвентарь лучше, чем отсутствие отчёта: health
                    // должен работать и когда winget недоступен.
                    tracing::warn!(error = %error, event = "SOFTWARE_INVENTORY_FAILED");
                }
            }
        }
        cache.software.clone()
    }

    /// Собирает отчёт. `policy_apply_failed` и `no_interactive_session` знает
    /// не сборщик метрик, а сам Service.
    pub fn report(
        &self,
        device_id: &str,
        policy_apply_failed: bool,
        no_interactive_session: bool,
        force_inventory: bool,
    ) -> protocol::network::DeviceHealthReport {
        let metrics = self.metrics.collect().unwrap_or_else(|error| {
            tracing::warn!(error = %error, event = "HEALTH_METRICS_FAILED");
            HealthMetrics::default()
        });
        let software = self.software(force_inventory);
        let drifted = profile(&self.profile_id)
            .map(|value| drift(value, &software))
            .unwrap_or_default();

        let facts = AgentFacts {
            policy_apply_failed,
            no_interactive_session,
            missing_software: drifted
                .iter()
                .filter(|entry| entry.kind == software_manager::DriftKind::Missing)
                .map(|entry| entry.application_id.clone())
                .collect(),
            mismatched_software: drifted
                .iter()
                .filter(|entry| entry.kind == software_manager::DriftKind::VersionMismatch)
                .map(|entry| entry.application_id.clone())
                .collect(),
        };
        let HealthAssessment { state, warnings } = device_health::assess(&metrics, &facts);

        protocol::network::DeviceHealthReport {
            device_id: device_id.to_owned(),
            state: to_proto_state(state) as i32,
            cpu_percent: metrics.cpu_percent,
            ram_percent: metrics.ram_percent,
            disk_percent: metrics.disk_percent,
            os_version: metrics.os_version,
            agent_version: env!("CARGO_PKG_VERSION").to_owned(),
            warnings,
            reported_at_unix_ms: now_unix_ms(),
            uptime_seconds: metrics.uptime_seconds,
            software: software
                .into_iter()
                .map(
                    |(application_id, version)| protocol::network::InstalledApplication {
                        application_id,
                        version,
                    },
                )
                .collect(),
            drift: drifted
                .into_iter()
                .map(|entry| protocol::network::SoftwareDrift {
                    application_id: entry.application_id,
                    kind: to_proto_drift(entry.kind) as i32,
                    required_version: entry.required_version,
                    actual_version: entry.actual_version,
                })
                .collect(),
            profile_id: self.profile_id.clone(),
        }
    }

    /// Приводит устройство к desired state профиля.
    ///
    /// Профиль, отличный от настроенного на устройстве, отклоняется: Repair не
    /// должен становиться способом поставить произвольный набор пакетов.
    pub fn repair(&self, profile_id: &str) -> Result<Vec<RepairItem>, SoftwareError> {
        if profile_id != self.profile_id {
            return Err(SoftwareError::UnknownProfile(profile_id.to_owned()));
        }
        let software = self.software(true);
        let items = repair_profile(&self.packages, profile_id, &software)?;
        // После установки кэш заведомо устарел.
        if let Ok(mut cache) = self.inventory.lock() {
            cache.refreshed_at = None;
        }
        Ok(items)
    }
}

fn to_proto_state(state: device_health::HealthState) -> protocol::network::DeviceHealthState {
    match state {
        device_health::HealthState::Healthy => protocol::network::DeviceHealthState::Healthy,
        device_health::HealthState::Warning => protocol::network::DeviceHealthState::Warning,
        device_health::HealthState::Critical => protocol::network::DeviceHealthState::Critical,
    }
}

fn to_proto_drift(kind: software_manager::DriftKind) -> protocol::network::DriftKind {
    match kind {
        software_manager::DriftKind::Missing => protocol::network::DriftKind::Missing,
        software_manager::DriftKind::VersionMismatch => {
            protocol::network::DriftKind::VersionMismatch
        }
    }
}

fn now_unix_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(i64::MAX)
}
