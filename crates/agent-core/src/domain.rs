//! Независимые от ОС доменные типы для trait'ов и supervisor.

use std::ffi::OsString;
use std::path::PathBuf;

/// Обнаруженная Windows session. Политику выбора определяет вызывающий код.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub session_id: u32,
}

/// Описание процесса для запуска внутри session.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
}

/// Процесс, запущенный и отслеживаемый supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedProcess {
    pub session_id: u32,
    pub pid: u32,
}
