//! Локальный расчёт состояния устройства.
//!
//! Health считается **на устройстве**, а не только в облаке: преподаватель
//! должен видеть состояние класса без облака (local-first, ADR-0005).
//! Сбор самих метрик скрыт за трейтом [`MetricsProvider`], поэтому правила
//! тестируются на любом хосте.

/// Машиночитаемые коды предупреждений.
///
/// Свободный текст здесь недопустим: коды попадают в UI и в автоматизацию,
/// а произвольная строка с устройства — плохой источник для обоих
/// (spec T7 §10.3).
pub mod warning {
    pub const DISK_SPACE_LOW: &str = "DISK_SPACE_LOW";
    pub const DISK_SPACE_CRITICAL: &str = "DISK_SPACE_CRITICAL";
    pub const MEMORY_PRESSURE: &str = "MEMORY_PRESSURE";
    pub const CPU_SATURATED: &str = "CPU_SATURATED";
    pub const SOFTWARE_MISSING: &str = "SOFTWARE_MISSING";
    pub const SOFTWARE_VERSION_MISMATCH: &str = "SOFTWARE_VERSION_MISMATCH";
    /// Менеджер пакетов недоступен: состав ПО неизвестен.
    ///
    /// Отдельный код от `SOFTWARE_MISSING` намеренно: «программы нет» и
    /// «мы не смогли посмотреть, есть ли программа» требуют от администратора
    /// разных действий, а объединение их в один код превращает исправную
    /// машину без winget в машину без программ.
    pub const SOFTWARE_MANAGER_UNAVAILABLE: &str = "SOFTWARE_MANAGER_UNAVAILABLE";
    pub const POLICY_APPLY_FAILED: &str = "POLICY_APPLY_FAILED";
    pub const NO_INTERACTIVE_SESSION: &str = "NO_INTERACTIVE_SESSION";
}

/// Пороги. Вынесены в константы, чтобы правило можно было прочитать, а не
/// вылавливать число в коде.
pub const DISK_WARNING_PERCENT: f64 = 90.0;
pub const DISK_CRITICAL_PERCENT: f64 = 97.0;
pub const MEMORY_WARNING_PERCENT: f64 = 92.0;
pub const CPU_WARNING_PERCENT: f64 = 95.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HealthState {
    Healthy,
    Warning,
    Critical,
}

/// Сырые метрики устройства.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HealthMetrics {
    pub cpu_percent: f64,
    pub ram_percent: f64,
    pub disk_percent: f64,
    pub uptime_seconds: i64,
    pub os_version: String,
}

/// Факты, которые знает не сборщик метрик, а сам агент.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentFacts {
    /// Последнее применение политики завершилось ошибкой.
    pub policy_apply_failed: bool,
    /// Нет интерактивной сессии (на ученической машине это ненормально).
    pub no_interactive_session: bool,
    /// Расхождения software-профиля: (application_id, версия требуется).
    pub missing_software: Vec<String>,
    pub mismatched_software: Vec<String>,
    /// Менеджер пакетов недоступен, поэтому состав ПО не проверялся.
    pub software_manager_unavailable: bool,
}

/// Результат расчёта.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthAssessment {
    pub state: HealthState,
    pub warnings: Vec<String>,
}

/// Правила из spec T7 §4.2.
///
/// Осознанно: нехватка ПО — это Warning (урок ещё можно вести), а сбой
/// применения политики — Critical, потому что устройство находится в
/// неизвестном состоянии защиты.
pub fn assess(metrics: &HealthMetrics, facts: &AgentFacts) -> HealthAssessment {
    let mut warnings = Vec::new();
    let mut state = HealthState::Healthy;

    let mut raise = |level: HealthState, code: &str, warnings: &mut Vec<String>| {
        warnings.push(code.to_owned());
        if level > state {
            state = level;
        }
    };

    if metrics.disk_percent >= DISK_CRITICAL_PERCENT {
        raise(
            HealthState::Critical,
            warning::DISK_SPACE_CRITICAL,
            &mut warnings,
        );
    } else if metrics.disk_percent > DISK_WARNING_PERCENT {
        raise(HealthState::Warning, warning::DISK_SPACE_LOW, &mut warnings);
    }
    if metrics.ram_percent > MEMORY_WARNING_PERCENT {
        raise(
            HealthState::Warning,
            warning::MEMORY_PRESSURE,
            &mut warnings,
        );
    }
    if metrics.cpu_percent > CPU_WARNING_PERCENT {
        raise(HealthState::Warning, warning::CPU_SATURATED, &mut warnings);
    }
    // Недоступный менеджер пакетов исключает разговор о составе ПО: списки
    // расхождений при нём заведомо пусты, и молчать об этом нельзя.
    if facts.software_manager_unavailable {
        raise(
            HealthState::Warning,
            warning::SOFTWARE_MANAGER_UNAVAILABLE,
            &mut warnings,
        );
    }
    if !facts.missing_software.is_empty() {
        raise(
            HealthState::Warning,
            warning::SOFTWARE_MISSING,
            &mut warnings,
        );
    }
    if !facts.mismatched_software.is_empty() {
        raise(
            HealthState::Warning,
            warning::SOFTWARE_VERSION_MISMATCH,
            &mut warnings,
        );
    }
    if facts.no_interactive_session {
        raise(
            HealthState::Warning,
            warning::NO_INTERACTIVE_SESSION,
            &mut warnings,
        );
    }
    if facts.policy_apply_failed {
        raise(
            HealthState::Critical,
            warning::POLICY_APPLY_FAILED,
            &mut warnings,
        );
    }

    HealthAssessment { state, warnings }
}

/// Источник метрик устройства.
pub trait MetricsProvider {
    fn collect(&self) -> Result<HealthMetrics, HealthError>;
}

#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error("сбор метрик: {0}")]
    Collect(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(cpu: f64, ram: f64, disk: f64) -> HealthMetrics {
        HealthMetrics {
            cpu_percent: cpu,
            ram_percent: ram,
            disk_percent: disk,
            uptime_seconds: 3_600,
            os_version: "Windows 11 23H2".to_owned(),
        }
    }

    #[test]
    fn healthy_device_has_no_warnings() {
        let result = assess(&metrics(20.0, 40.0, 55.0), &AgentFacts::default());
        assert_eq!(result.state, HealthState::Healthy);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn disk_above_ninety_percent_is_warning() {
        let result = assess(&metrics(10.0, 30.0, 91.0), &AgentFacts::default());
        assert_eq!(result.state, HealthState::Warning);
        assert_eq!(result.warnings, vec![warning::DISK_SPACE_LOW]);
    }

    #[test]
    fn exactly_ninety_percent_is_still_healthy() {
        // Правило спеки — "disk > 90%", а не ">=".
        let result = assess(&metrics(10.0, 30.0, 90.0), &AgentFacts::default());
        assert_eq!(result.state, HealthState::Healthy);
    }

    #[test]
    fn nearly_full_disk_escalates_to_critical() {
        let result = assess(&metrics(10.0, 30.0, 98.0), &AgentFacts::default());
        assert_eq!(result.state, HealthState::Critical);
        assert_eq!(result.warnings, vec![warning::DISK_SPACE_CRITICAL]);
    }

    #[test]
    fn missing_package_is_warning() {
        let facts = AgentFacts {
            missing_software: vec!["python".to_owned()],
            ..AgentFacts::default()
        };
        let result = assess(&metrics(10.0, 30.0, 50.0), &facts);
        assert_eq!(result.state, HealthState::Warning);
        assert_eq!(result.warnings, vec![warning::SOFTWARE_MISSING]);
    }

    #[test]
    fn policy_failure_is_critical_even_on_a_healthy_machine() {
        let facts = AgentFacts {
            policy_apply_failed: true,
            ..AgentFacts::default()
        };
        let result = assess(&metrics(5.0, 10.0, 20.0), &facts);
        assert_eq!(result.state, HealthState::Critical);
    }

    #[test]
    fn combined_problems_keep_the_worst_state_and_report_every_code() {
        let facts = AgentFacts {
            policy_apply_failed: true,
            missing_software: vec!["git".to_owned()],
            mismatched_software: vec!["python".to_owned()],
            no_interactive_session: true,
            software_manager_unavailable: false,
        };
        let result = assess(&metrics(99.0, 95.0, 99.0), &facts);

        assert_eq!(result.state, HealthState::Critical);
        for expected in [
            warning::DISK_SPACE_CRITICAL,
            warning::MEMORY_PRESSURE,
            warning::CPU_SATURATED,
            warning::SOFTWARE_MISSING,
            warning::SOFTWARE_VERSION_MISMATCH,
            warning::NO_INTERACTIVE_SESSION,
            warning::POLICY_APPLY_FAILED,
        ] {
            assert!(
                result.warnings.iter().any(|code| code == expected),
                "нет кода {expected} в {:?}",
                result.warnings
            );
        }
    }

    #[test]
    fn warning_does_not_downgrade_critical() {
        let facts = AgentFacts {
            policy_apply_failed: true,
            missing_software: vec!["git".to_owned()],
            ..AgentFacts::default()
        };
        // Порядок правил не должен влиять на итог.
        assert_eq!(
            assess(&metrics(99.0, 10.0, 10.0), &facts).state,
            HealthState::Critical
        );
    }

    /// Машина без менеджера пакетов исправна: неизвестен состав ПО, а не
    /// отсутствуют программы. Иначе администратор чинил бы не то.
    #[test]
    fn missing_package_manager_is_not_missing_software() {
        let facts = AgentFacts {
            software_manager_unavailable: true,
            ..AgentFacts::default()
        };
        let result = assess(&metrics(5.0, 10.0, 20.0), &facts);

        assert_eq!(result.state, HealthState::Warning);
        assert_eq!(
            result.warnings,
            vec![warning::SOFTWARE_MANAGER_UNAVAILABLE.to_owned()]
        );
        assert!(
            !result
                .warnings
                .contains(&warning::SOFTWARE_MISSING.to_owned())
        );
    }
}
