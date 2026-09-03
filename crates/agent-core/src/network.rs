//! Независимая от транспорта доменная логика сетевого milestone T1.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use uuid::Uuid;

/// Интервал heartbeat Teacher ↔ Agent.
pub const NETWORK_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// После этого интервала без сообщений устройство считается отключённым.
pub const NETWORK_OFFLINE_TIMEOUT: Duration = Duration::from_secs(15);
/// Срок действия enrollment-кода по умолчанию.
pub const DEFAULT_ENROLLMENT_TTL: Duration = Duration::from_secs(10 * 60);

/// Контекст, к которому привязан enrollment-код.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentContext {
    pub organization_id: String,
    pub branch_id: String,
}

impl Default for EnrollmentContext {
    fn default() -> Self {
        Self {
            organization_id: "default".to_owned(),
            branch_id: "default".to_owned(),
        }
    }
}

/// Выданный оператору короткоживущий код.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentCode {
    pub value: String,
    pub expires_at_unix_ms: i64,
    pub context: EnrollmentContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EnrollmentCodeError {
    #[error("enrollment-код не найден")]
    Invalid,
    #[error("срок действия enrollment-кода истёк")]
    Expired,
    #[error("enrollment-код уже использован")]
    AlreadyUsed,
    #[error("контекст enrollment-кода не совпадает")]
    ContextMismatch,
}

/// Локальный issuer одноразовых enrollment-кодов для Teacher Console T1.
#[derive(Debug, Default)]
pub struct EnrollmentAuthority {
    pending: HashMap<String, EnrollmentCode>,
    used: HashSet<String>,
}

impl EnrollmentAuthority {
    /// Создаёт код с заданным TTL. В production вызывающая сторона обязана
    /// использовать положительный TTL и текущее время Unix.
    pub fn issue(
        &mut self,
        context: EnrollmentContext,
        now_unix_ms: i64,
        ttl: Duration,
    ) -> EnrollmentCode {
        let value = Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(12)
            .collect::<String>()
            .to_uppercase();
        let ttl_ms = i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX);
        let code = EnrollmentCode {
            value: value.clone(),
            expires_at_unix_ms: now_unix_ms.saturating_add(ttl_ms),
            context,
        };
        self.pending.insert(value, code.clone());
        code
    }

    /// Атомарно проверяет и поглощает код. Успешно использованный код нельзя
    /// применить повторно, даже если его TTL ещё не истёк.
    pub fn consume(
        &mut self,
        value: &str,
        context: &EnrollmentContext,
        now_unix_ms: i64,
    ) -> Result<(), EnrollmentCodeError> {
        let normalized = value.trim().to_uppercase();
        if self.used.contains(&normalized) {
            return Err(EnrollmentCodeError::AlreadyUsed);
        }

        let Some(code) = self.pending.get(&normalized) else {
            return Err(EnrollmentCodeError::Invalid);
        };
        if now_unix_ms >= code.expires_at_unix_ms {
            self.pending.remove(&normalized);
            return Err(EnrollmentCodeError::Expired);
        }
        if &code.context != context {
            return Err(EnrollmentCodeError::ContextMismatch);
        }

        self.pending.remove(&normalized);
        self.used.insert(normalized);
        Ok(())
    }
}

/// Состояния соединения Teacher Console с одним устройством из T1 spec §9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Discovered,
    Connecting,
    Authenticating,
    Connected,
    Degraded,
    Disconnected,
    Unauthorized,
    UpgradeRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("недопустимый переход соединения: {from:?} -> {to:?}")]
pub struct InvalidConnectionTransition {
    pub from: ConnectionState,
    pub to: ConnectionState,
}

impl ConnectionState {
    /// Проверяет переход, не позволяя UI изобразить состояние, невозможное в
    /// сетевом lifecycle.
    pub fn transition(self, next: Self) -> Result<Self, InvalidConnectionTransition> {
        use ConnectionState as S;
        let allowed = matches!(
            (self, next),
            (S::Discovered, S::Connecting)
                | (
                    S::Connecting,
                    S::Authenticating | S::Disconnected | S::Degraded
                )
                | (
                    S::Authenticating,
                    S::Connected | S::Unauthorized | S::UpgradeRequired | S::Disconnected
                )
                | (S::Connected, S::Degraded | S::Disconnected)
                | (S::Degraded, S::Connected | S::Disconnected | S::Connecting)
                | (S::Disconnected, S::Connecting | S::Discovered)
                | (S::Unauthorized, S::Authenticating | S::Disconnected)
                | (S::UpgradeRequired, S::Disconnected)
        );
        allowed.then_some(next).ok_or(InvalidConnectionTransition {
            from: self,
            to: next,
        })
    }
}

/// Определяет offline только по последнему наблюдаемому сообщению.
pub fn is_offline(last_seen_unix_ms: i64, now_unix_ms: i64, timeout: Duration) -> bool {
    let timeout_ms = i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX);
    now_unix_ms.saturating_sub(last_seen_unix_ms) > timeout_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrollment_code_is_one_time() {
        let mut authority = EnrollmentAuthority::default();
        let context = EnrollmentContext::default();
        let code = authority.issue(context.clone(), 1_000, DEFAULT_ENROLLMENT_TTL);

        assert_eq!(authority.consume(&code.value, &context, 2_000), Ok(()));
        assert_eq!(
            authority.consume(&code.value, &context, 2_001),
            Err(EnrollmentCodeError::AlreadyUsed)
        );
    }

    #[test]
    fn expired_enrollment_code_is_rejected() {
        let mut authority = EnrollmentAuthority::default();
        let context = EnrollmentContext::default();
        let code = authority.issue(context.clone(), 1_000, Duration::from_millis(500));
        assert_eq!(
            authority.consume(&code.value, &context, 1_500),
            Err(EnrollmentCodeError::Expired)
        );
    }

    #[test]
    fn context_mismatch_does_not_consume_code() {
        let mut authority = EnrollmentAuthority::default();
        let context = EnrollmentContext::default();
        let code = authority.issue(context.clone(), 1_000, DEFAULT_ENROLLMENT_TTL);
        let wrong_context = EnrollmentContext {
            organization_id: "other".to_owned(),
            branch_id: "default".to_owned(),
        };

        assert_eq!(
            authority.consume(&code.value, &wrong_context, 2_000),
            Err(EnrollmentCodeError::ContextMismatch)
        );
        assert_eq!(authority.consume(&code.value, &context, 2_000), Ok(()));
    }

    #[test]
    fn connection_state_machine_accepts_happy_path() {
        let state = ConnectionState::Discovered
            .transition(ConnectionState::Connecting)
            .unwrap()
            .transition(ConnectionState::Authenticating)
            .unwrap()
            .transition(ConnectionState::Connected)
            .unwrap();
        assert_eq!(state, ConnectionState::Connected);
    }

    #[test]
    fn connection_state_machine_rejects_trust_without_authentication() {
        assert!(
            ConnectionState::Discovered
                .transition(ConnectionState::Connected)
                .is_err()
        );
    }

    #[test]
    fn offline_timeout_uses_strict_boundary() {
        assert!(!is_offline(1_000, 16_000, NETWORK_OFFLINE_TIMEOUT));
        assert!(is_offline(1_000, 16_001, NETWORK_OFFLINE_TIMEOUT));
    }
}
