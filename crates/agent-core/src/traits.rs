//! Trait-абстракции над Windows-примитивами. Бизнес-логика supervisor
//! зависит только от них и тестируется с mock без вызовов Win32.

use crate::domain::{ManagedProcess, ProcessSpec, Session};
use crate::error::Result;
use protocol::Envelope;

/// Находит активную интерактивную session.
pub trait SessionProvider: Send + Sync {
    fn active_console_session(&self) -> Result<Option<Session>>;
}

/// Запускает процесс внутри session и управляет им.
pub trait SessionProcessLauncher: Send + Sync {
    /// Запускает `spec` внутри `session_id`.
    fn launch(&self, session_id: u32, spec: &ProcessSpec) -> Result<ManagedProcess>;

    /// Проверяет, жив ли процесс с указанным PID.
    fn is_alive(&self, pid: u32) -> bool;

    /// Завершает только процесс, PID которого вернул этот launcher.
    fn terminate(&self, pid: u32) -> Result<()>;
}

/// Одно принятое локальное IPC-соединение.
#[async_trait::async_trait]
pub trait LocalIpcConnection: Send {
    async fn send(&mut self, envelope: &Envelope) -> Result<()>;

    /// Возвращает `Ok(None)` при штатном закрытии соединения.
    async fn recv(&mut self) -> Result<Option<Envelope>>;

    /// Windows session id соединения. Сервер проверяет его через WinAPI,
    /// не доверяя данным клиента.
    fn peer_session_id(&self) -> Option<u32>;

    /// PID клиента по данным ОС.
    fn peer_pid(&self) -> Option<u32>;
}

/// Принимает локальные IPC-соединения.
#[async_trait::async_trait]
pub trait LocalIpcServer: Send + Sync {
    async fn accept(&self) -> Result<Box<dyn LocalIpcConnection>>;
}
