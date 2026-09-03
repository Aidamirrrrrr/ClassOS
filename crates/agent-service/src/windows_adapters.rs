//! Реализации `SessionProvider` и `SessionProcessLauncher` поверх Win32.
//! Адаптируют примитивы `windows-platform` к независимым от ОС trait'ам
//! `SessionSupervisor` (спека §142-146). Только Windows.
//!
//! `WindowsProcessLauncher` также создаёт имя Named Pipe с новым
//! `session_instance_id` и передаёт его Session Host через аргументы.
//! Runtime получает имя для PID через `pipe_name_for`.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use agent_core::domain::{ManagedProcess, ProcessSpec, Session};
use agent_core::error::{AgentError, Result};
use agent_core::traits::{SessionProcessLauncher, SessionProvider};
use windows_platform::handles::OwnedHandle;

/// Находит физическую console session через `WTSGetActiveConsoleSessionId`.
pub struct WindowsSessionProvider;

impl SessionProvider for WindowsSessionProvider {
    fn active_console_session(&self) -> Result<Option<Session>> {
        Ok(windows_platform::sessions::active_console_session_id()
            .map(|session_id| Session { session_id }))
    }
}

/// Выбирает привилегированный запуск через `CreateProcessAsUserW` либо
/// обычный дочерний процесс текущего пользователя в dev-режиме.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    Privileged,
    DevChildProcess,
}

/// Запускает Session Host и хранит его handles, чтобы проверять и завершать
/// только процессы, созданные этим launcher (спека §72-74).
pub struct WindowsProcessLauncher {
    mode: LaunchMode,
    session_host_path: PathBuf,
    handles: Mutex<HashMap<u32, OwnedHandle>>,
    dev_children: Mutex<HashMap<u32, std::process::Child>>,
    pipe_names: Mutex<HashMap<u32, String>>,
}

impl WindowsProcessLauncher {
    pub fn new(session_host_path: PathBuf, mode: LaunchMode) -> Self {
        Self {
            mode,
            session_host_path,
            handles: Mutex::new(HashMap::new()),
            dev_children: Mutex::new(HashMap::new()),
            pipe_names: Mutex::new(HashMap::new()),
        }
    }

    /// Имя Named Pipe для запуска с указанным PID, если он ещё отслеживается.
    pub fn pipe_name_for(&self, pid: u32) -> Option<String> {
        self.pipe_names
            .lock()
            .expect("WindowsProcessLauncher mutex poisoned")
            .get(&pid)
            .cloned()
    }

    fn build_args(session_id: u32, pipe_name: &str) -> Vec<OsString> {
        vec![
            OsString::from("--session-id"),
            OsString::from(session_id.to_string()),
            OsString::from("--pipe"),
            OsString::from(pipe_name),
        ]
    }

    fn launch_privileged(&self, session_id: u32, pipe_name: &str) -> Result<ManagedProcess> {
        let args = Self::build_args(session_id, pipe_name);
        let launched = windows_platform::process::launch_in_session(
            session_id,
            &self.session_host_path,
            &args,
        )
        .map_err(|err| AgentError::ProcessLaunchFailed {
            session_id,
            reason: err.to_string(),
        })?;

        let pid = launched.pid;
        self.handles
            .lock()
            .expect("WindowsProcessLauncher mutex poisoned")
            .insert(pid, launched.process_handle);
        Ok(ManagedProcess { session_id, pid })
    }

    fn launch_dev_child(&self, session_id: u32, pipe_name: &str) -> Result<ManagedProcess> {
        let args = Self::build_args(session_id, pipe_name);
        let child = std::process::Command::new(&self.session_host_path)
            .args(&args)
            .spawn()
            .map_err(|err| AgentError::ProcessLaunchFailed {
                session_id,
                reason: err.to_string(),
            })?;
        let pid = child.id();
        self.dev_children
            .lock()
            .expect("WindowsProcessLauncher mutex poisoned")
            .insert(pid, child);
        Ok(ManagedProcess { session_id, pid })
    }
}

impl SessionProcessLauncher for WindowsProcessLauncher {
    fn launch(&self, session_id: u32, _spec: &ProcessSpec) -> Result<ManagedProcess> {
        let instance_id = uuid::Uuid::new_v4();
        let pipe_name = windows_platform::pipes::pipe_name(session_id, &instance_id.to_string());

        let managed = match self.mode {
            LaunchMode::Privileged => self.launch_privileged(session_id, &pipe_name)?,
            LaunchMode::DevChildProcess => self.launch_dev_child(session_id, &pipe_name)?,
        };

        self.pipe_names
            .lock()
            .expect("WindowsProcessLauncher mutex poisoned")
            .insert(managed.pid, pipe_name);
        Ok(managed)
    }

    fn is_alive(&self, pid: u32) -> bool {
        match self.mode {
            LaunchMode::Privileged => {
                let handles = self
                    .handles
                    .lock()
                    .expect("WindowsProcessLauncher mutex poisoned");
                match handles.get(&pid) {
                    Some(handle) => {
                        windows_platform::process::is_process_alive(handle).unwrap_or(false)
                    }
                    None => false,
                }
            }
            LaunchMode::DevChildProcess => {
                let mut children = self
                    .dev_children
                    .lock()
                    .expect("WindowsProcessLauncher mutex poisoned");
                match children.get_mut(&pid) {
                    Some(child) => matches!(child.try_wait(), Ok(None)),
                    None => false,
                }
            }
        }
    }

    fn terminate(&self, pid: u32) -> Result<()> {
        self.pipe_names
            .lock()
            .expect("WindowsProcessLauncher mutex poisoned")
            .remove(&pid);

        match self.mode {
            LaunchMode::Privileged => {
                let mut handles = self
                    .handles
                    .lock()
                    .expect("WindowsProcessLauncher mutex poisoned");
                if let Some(handle) = handles.remove(&pid) {
                    windows_platform::process::terminate_process(&handle).map_err(|err| {
                        AgentError::ProcessLaunchFailed {
                            session_id: 0,
                            reason: err.to_string(),
                        }
                    })?;
                }
            }
            LaunchMode::DevChildProcess => {
                let mut children = self
                    .dev_children
                    .lock()
                    .expect("WindowsProcessLauncher mutex poisoned");
                if let Some(mut child) = children.remove(&pid) {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        Ok(())
    }
}

/// Получает SDDL SID пользователя `session_id` для построения ACL канала.
/// Используется только в привилегированном режиме.
pub fn user_sid_for_session(session_id: u32) -> Result<String> {
    let token = windows_platform::process::query_user_token(session_id).map_err(|err| {
        tracing::warn!(session_id, error = %err, "query_user_token failed");
        AgentError::UserTokenFailed { session_id }
    })?;
    windows_platform::security::user_sid_string(token.raw()).map_err(|err| {
        tracing::warn!(session_id, error = %err, "user_sid_string failed");
        AgentError::UserTokenFailed { session_id }
    })
}

/// Находит Session Host рядом с `classos-service.exe`. Запуск разрешён
/// только из доверенного каталога установки ClassOS.
pub fn default_session_host_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().map_err(AgentError::Io)?;
    let dir = exe.parent().ok_or_else(|| AgentError::Config {
        reason: "classos-service.exe has no parent directory".to_string(),
    })?;
    Ok(session_host_binary_path(dir))
}

fn session_host_binary_path(dir: &Path) -> PathBuf {
    dir.join("classos-session.exe")
}
