//! Mock-реализации trait'ов для тестирования state machine без Win32.

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::domain::{ManagedProcess, ProcessSpec, Session};
use crate::error::{AgentError, Result};
use crate::traits::SessionProvider;

/// `SessionProvider`, активную session которого тест может изменить.
pub struct MockSessionProvider {
    active: Mutex<Option<u32>>,
}

impl MockSessionProvider {
    pub fn new(active_session_id: Option<u32>) -> Self {
        Self {
            active: Mutex::new(active_session_id),
        }
    }

    pub fn set_active(&self, session_id: Option<u32>) {
        *self.active.lock().expect("mock mutex poisoned") = session_id;
    }
}

impl SessionProvider for MockSessionProvider {
    fn active_console_session(&self) -> Result<Option<Session>> {
        Ok(self
            .active
            .lock()
            .expect("mock mutex poisoned")
            .map(|session_id| Session { session_id }))
    }
}

enum LaunchMode {
    Normal,
    AlwaysFailLaunch,
    AlwaysDeadAfterLaunch,
}

/// Launcher, который хранит живые PID в памяти вместо вызовов Win32.
pub struct MockProcessLauncher {
    next_pid: AtomicU32,
    alive: Mutex<HashSet<u32>>,
    mode: LaunchMode,
}

impl MockProcessLauncher {
    pub fn new() -> Self {
        Self {
            next_pid: AtomicU32::new(1000),
            alive: Mutex::new(HashSet::new()),
            mode: LaunchMode::Normal,
        }
    }

    pub fn always_fail_launch() -> Self {
        Self {
            mode: LaunchMode::AlwaysFailLaunch,
            ..Self::new()
        }
    }

    /// Каждый процесс сразу считается завершённым для проверки crash-loop.
    pub fn always_dead_after_launch() -> Self {
        Self {
            mode: LaunchMode::AlwaysDeadAfterLaunch,
            ..Self::new()
        }
    }

    /// Тестовый helper: помечает PID завершённым, имитируя crash.
    pub fn kill(&self, pid: u32) {
        self.alive.lock().expect("mock mutex poisoned").remove(&pid);
    }
}

impl Default for MockProcessLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::traits::SessionProcessLauncher for MockProcessLauncher {
    fn launch(&self, session_id: u32, _spec: &ProcessSpec) -> Result<ManagedProcess> {
        if matches!(self.mode, LaunchMode::AlwaysFailLaunch) {
            return Err(AgentError::ProcessLaunchFailed {
                session_id,
                reason: "mock configured to always fail".to_string(),
            });
        }

        let pid = self.next_pid.fetch_add(1, Ordering::SeqCst);
        if !matches!(self.mode, LaunchMode::AlwaysDeadAfterLaunch) {
            self.alive.lock().expect("mock mutex poisoned").insert(pid);
        }
        Ok(ManagedProcess { session_id, pid })
    }

    fn is_alive(&self, pid: u32) -> bool {
        self.alive
            .lock()
            .expect("mock mutex poisoned")
            .contains(&pid)
    }

    fn terminate(&self, pid: u32) -> Result<()> {
        self.alive.lock().expect("mock mutex poisoned").remove(&pid);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::SessionProcessLauncher;

    #[test]
    fn mock_provider_reports_configured_session() {
        let provider = MockSessionProvider::new(Some(7));
        assert_eq!(
            provider.active_console_session().unwrap(),
            Some(Session { session_id: 7 })
        );
        provider.set_active(None);
        assert_eq!(provider.active_console_session().unwrap(), None);
    }

    #[test]
    fn mock_launcher_tracks_liveness() {
        let launcher = MockProcessLauncher::new();
        let spec = ProcessSpec {
            executable: "x".into(),
            args: vec![],
        };
        let managed = launcher.launch(1, &spec).unwrap();
        assert!(launcher.is_alive(managed.pid));
        launcher.terminate(managed.pid).unwrap();
        assert!(!launcher.is_alive(managed.pid));
    }
}
