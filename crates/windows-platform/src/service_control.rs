//! Управление службой ClassOS через SCM-утилиту `sc.exe`.
//!
//! Нужен обновлятору: служба не может остановить и заменить сама себя вживую
//! (spec T8 §8.4), поэтому это делает отдельный процесс.

use std::process::Command;
use std::time::{Duration, Instant};

use crate::{PlatformError, Result};

pub const SERVICE_NAME: &str = "ClassOSAgent";

fn run_sc(args: &[&str]) -> Result<String> {
    let output = Command::new("sc.exe")
        .args(args)
        .output()
        .map_err(|error| PlatformError::Unexpected {
            api: "sc.exe",
            reason: error.to_string(),
        })?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Состояние службы по данным SCM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Running,
    Stopped,
    Pending,
    Unknown,
}

pub fn query_state() -> Result<ServiceState> {
    Ok(parse_state(&run_sc(&["query", SERVICE_NAME])?))
}

fn parse_state(output: &str) -> ServiceState {
    let state_line = output
        .lines()
        .find(|line| line.trim_start().starts_with("STATE"));
    match state_line {
        Some(line) if line.contains("RUNNING") => ServiceState::Running,
        Some(line) if line.contains("STOPPED") => ServiceState::Stopped,
        Some(line) if line.contains("PENDING") => ServiceState::Pending,
        _ => ServiceState::Unknown,
    }
}

pub fn stop() -> Result<()> {
    run_sc(&["stop", SERVICE_NAME]).map(|_| ())
}

pub fn start() -> Result<()> {
    run_sc(&["start", SERVICE_NAME]).map(|_| ())
}

/// Ждёт нужного состояния службы.
///
/// SCM отвечает на команду раньше, чем служба действительно перешла в
/// состояние, поэтому опрос обязателен: без него обновлятор заменил бы файлы
/// работающей службы.
pub fn wait_for(expected: ServiceState, timeout: Duration) -> Result<bool> {
    let deadline = Instant::now() + timeout;
    loop {
        if query_state()? == expected {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_running_state() {
        let output = "SERVICE_NAME: ClassOSAgent\n        STATE              : 4  RUNNING\n";
        assert_eq!(parse_state(output), ServiceState::Running);
    }

    #[test]
    fn parses_stopped_and_pending_states() {
        assert_eq!(
            parse_state("        STATE              : 1  STOPPED\n"),
            ServiceState::Stopped
        );
        assert_eq!(
            parse_state("        STATE              : 3  STOP_PENDING\n"),
            ServiceState::Pending
        );
    }

    #[test]
    fn missing_service_is_unknown_not_stopped() {
        // Иначе обновлятор решил бы, что служба успешно остановлена.
        assert_eq!(
            parse_state("The specified service does not exist.\n"),
            ServiceState::Unknown
        );
    }
}
