//! [`SessionSupervisor`]: основной reconcile-цикл desired state для T0.
//! Зависит только от trait'ов и тестируется с mock без Win32.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::domain::ProcessSpec;
use crate::error::Result;
use crate::traits::{SessionProcessLauncher, SessionProvider};

/// События, которые Windows Service передаёт из обработчика SCM. Они
/// запускают внеочередной reconcile и передают состояние lock/unlock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceEvent {
    Stop,
    Shutdown,

    SessionLogon(u32),
    SessionLogoff(u32),

    SessionLock(u32),
    SessionUnlock(u32),

    ConsoleConnect(u32),
    ConsoleDisconnect(u32),
}

/// Состояния state machine supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorState {
    NoInteractiveSession,
    Starting { session_id: u32 },
    WaitingForIpc { session_id: u32, pid: u32 },
    Running { session_id: u32, pid: u32 },
    Stopping { session_id: u32, pid: u32 },
    Backoff,
}

/// Наблюдаемые результаты одного reconcile для логов и unit-тестов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorEvent {
    SessionDiscovered {
        session_id: u32,
    },
    SessionHostStarting {
        session_id: u32,
    },
    SessionHostStarted {
        session_id: u32,
        pid: u32,
    },
    SessionHostLaunchFailed {
        session_id: u32,
        reason: String,
    },
    SessionHostExited {
        session_id: u32,
        pid: u32,
    },
    SessionHostRestarting {
        attempt: u32,
        delay: Duration,
    },
    SessionHostStopping {
        session_id: u32,
        pid: u32,
    },
    SessionChanged {
        old_session_id: u32,
        new_session_id: u32,
    },
    NoInteractiveSession,
    CrashLoopDetected {
        session_id: u32,
    },
}

/// Ступени exponential backoff в секундах с пределом 60 секунд.
pub const BACKOFF_STEPS_SECS: [u64; 6] = [1, 2, 5, 10, 30, 60];

/// Время стабильной работы, после которого backoff сбрасывается.
pub const STABLE_RUN_DURATION: Duration = Duration::from_secs(60);

/// Окно и порог обнаружения crash-loop.
pub const CRASH_LOOP_WINDOW: Duration = Duration::from_secs(120);
pub const CRASH_LOOP_THRESHOLD: usize = 5;

/// Предельное ожидание IPC с запасом на медленный запуск desktop.
pub const WAITING_FOR_IPC_TIMEOUT: Duration = Duration::from_secs(30);

/// Чистая функция расчёта задержки по номеру повторной попытки.
pub fn backoff_delay(attempt: u32) -> Duration {
    let idx = (attempt as usize).min(BACKOFF_STEPS_SECS.len() - 1);
    Duration::from_secs(BACKOFF_STEPS_SECS[idx])
}

/// Хранит недавние crash в скользящем окне.
#[derive(Debug, Default)]
struct CrashTracker {
    events: VecDeque<Instant>,
}

impl CrashTracker {
    fn record(&mut self, now: Instant) -> bool {
        self.events.push_back(now);
        while let Some(&front) = self.events.front() {
            if now.duration_since(front) > CRASH_LOOP_WINDOW {
                self.events.pop_front();
            } else {
                break;
            }
        }
        self.events.len() >= CRASH_LOOP_THRESHOLD
    }

    fn reset(&mut self) {
        self.events.clear();
    }
}

/// Supervisor T0 сравнивает desired state session с реальным процессом и
/// исправляет расхождения через launcher.
pub struct SessionSupervisor<P, L> {
    provider: P,
    launcher: L,
    session_executable: ProcessSpec,
    state: SupervisorState,
    backoff_attempt: u32,
    backoff_until: Option<Instant>,
    crash_tracker: CrashTracker,
    running_since: Option<Instant>,
    waiting_since: Option<Instant>,
}

impl<P, L> SessionSupervisor<P, L>
where
    P: SessionProvider,
    L: SessionProcessLauncher,
{
    pub fn new(provider: P, launcher: L, session_executable: ProcessSpec) -> Self {
        Self {
            provider,
            launcher,
            session_executable,
            state: SupervisorState::NoInteractiveSession,
            backoff_attempt: 0,
            backoff_until: None,
            crash_tracker: CrashTracker::default(),
            running_since: None,
            waiting_since: None,
        }
    }

    pub fn state(&self) -> SupervisorState {
        self.state
    }

    /// Read-only доступ к launcher для получения параметров запуска из IPC-слоя.
    pub fn launcher(&self) -> &L {
        &self.launcher
    }

    /// После проверенного handshake переводит совпадающий запуск в Running.
    pub fn notify_ipc_ready(&mut self, session_id: u32, pid: u32, now: Instant) {
        if let SupervisorState::WaitingForIpc {
            session_id: sid,
            pid: p,
        } = self.state
            && sid == session_id
            && p == pid
        {
            self.state = SupervisorState::Running { session_id, pid };
            self.running_since = Some(now);
            self.waiting_since = None;
        }
    }

    /// Помечает IPC нездоровым. Живой, но зависший Session Host завершается
    /// и перезапускается так же, как упавший процесс.
    pub fn notify_ipc_lost(&mut self, pid: u32, now: Instant) -> Vec<SupervisorEvent> {
        let (session_id, managed_pid) = match self.state {
            SupervisorState::WaitingForIpc { session_id, pid }
            | SupervisorState::Running { session_id, pid } => (session_id, pid),
            _ => return Vec::new(),
        };
        if managed_pid != pid {
            return Vec::new();
        }

        let mut events = vec![SupervisorEvent::SessionHostStopping { session_id, pid }];
        let _ = self.launcher.terminate(pid);
        events.push(SupervisorEvent::SessionHostExited { session_id, pid });
        self.running_since = None;
        self.waiting_since = None;
        self.enter_backoff(session_id, now, &mut events);
        events
    }

    /// Выполняет идемпотентный reconcile desired и фактического состояния.
    pub fn reconcile(&mut self, now: Instant) -> Result<Vec<SupervisorEvent>> {
        let mut events = Vec::new();
        let desired = self.provider.active_console_session()?;

        // Не перезапускаем процесс до истечения backoff.
        if let Some(until) = self.backoff_until {
            if now < until {
                return Ok(events);
            }
            self.backoff_until = None;
        }

        match (self.state, desired) {
            (SupervisorState::NoInteractiveSession, None) => {}

            (SupervisorState::NoInteractiveSession, Some(session)) => {
                events.push(SupervisorEvent::SessionDiscovered {
                    session_id: session.session_id,
                });
                self.start_session_host(session.session_id, now, &mut events);
            }

            (SupervisorState::Backoff, None) => {
                self.state = SupervisorState::NoInteractiveSession;
                self.crash_tracker.reset();
                self.backoff_attempt = 0;
            }

            (SupervisorState::Backoff, Some(session)) => {
                self.start_session_host(session.session_id, now, &mut events);
            }

            (SupervisorState::Starting { .. }, desired) => {
                // Starting должен быть переходным. Незавершённую попытку
                // сбрасываем и повторяем на следующем проходе.
                self.state = SupervisorState::NoInteractiveSession;
                if let Some(session) = desired {
                    self.start_session_host(session.session_id, now, &mut events);
                }
            }

            (SupervisorState::WaitingForIpc { session_id, pid }, desired) => {
                self.handle_active_process_state(session_id, pid, desired, now, &mut events, true);
            }

            (SupervisorState::Running { session_id, pid }, desired) => {
                self.handle_active_process_state(session_id, pid, desired, now, &mut events, false);
            }

            (SupervisorState::Stopping { session_id, pid }, _) => {
                // Завершаем переходное состояние Stopping.
                let _ = self.launcher.terminate(pid);
                events.push(SupervisorEvent::SessionHostExited { session_id, pid });
                self.state = SupervisorState::NoInteractiveSession;
                self.running_since = None;
                self.waiting_since = None;
                if let Some(session) = self.provider.active_console_session()? {
                    self.start_session_host(session.session_id, now, &mut events);
                }
            }
        }

        Ok(events)
    }

    fn handle_active_process_state(
        &mut self,
        session_id: u32,
        pid: u32,
        desired: Option<crate::domain::Session>,
        now: Instant,
        events: &mut Vec<SupervisorEvent>,
        is_waiting: bool,
    ) {
        match desired {
            None => {
                // После logout Session Host больше не нужен.
                events.push(SupervisorEvent::SessionHostStopping { session_id, pid });
                let _ = self.launcher.terminate(pid);
                events.push(SupervisorEvent::SessionHostExited { session_id, pid });
                self.state = SupervisorState::NoInteractiveSession;
                self.running_since = None;
                self.waiting_since = None;
            }
            Some(session) if session.session_id != session_id => {
                // При смене console session заменяем управляемый host.
                events.push(SupervisorEvent::SessionChanged {
                    old_session_id: session_id,
                    new_session_id: session.session_id,
                });
                let _ = self.launcher.terminate(pid);
                events.push(SupervisorEvent::SessionHostExited { session_id, pid });
                self.running_since = None;
                self.waiting_since = None;
                self.state = SupervisorState::NoInteractiveSession;
                self.start_session_host(session.session_id, now, events);
            }
            Some(_) if !self.launcher.is_alive(pid) => {
                // Процесс завершился аварийно.
                events.push(SupervisorEvent::SessionHostExited { session_id, pid });
                self.running_since = None;
                self.waiting_since = None;
                self.enter_backoff(session_id, now, events);
            }
            Some(_) if is_waiting => {
                if let Some(waiting_since) = self.waiting_since
                    && now.duration_since(waiting_since) > WAITING_FOR_IPC_TIMEOUT
                {
                    events.push(SupervisorEvent::SessionHostExited { session_id, pid });
                    let _ = self.launcher.terminate(pid);
                    self.waiting_since = None;
                    self.enter_backoff(session_id, now, events);
                }
                // Иначе продолжаем допустимое ожидание.
            }
            Some(_) => {
                // После стабильной работы сбрасываем backoff.
                if let Some(running_since) = self.running_since
                    && now.duration_since(running_since) >= STABLE_RUN_DURATION
                {
                    self.backoff_attempt = 0;
                    self.crash_tracker.reset();
                }
            }
        }
    }

    fn start_session_host(
        &mut self,
        session_id: u32,
        now: Instant,
        events: &mut Vec<SupervisorEvent>,
    ) {
        self.state = SupervisorState::Starting { session_id };
        events.push(SupervisorEvent::SessionHostStarting { session_id });

        match self.launcher.launch(session_id, &self.session_executable) {
            Ok(managed) => {
                self.state = SupervisorState::WaitingForIpc {
                    session_id,
                    pid: managed.pid,
                };
                self.waiting_since = Some(now);
                events.push(SupervisorEvent::SessionHostStarted {
                    session_id,
                    pid: managed.pid,
                });
            }
            Err(err) => {
                events.push(SupervisorEvent::SessionHostLaunchFailed {
                    session_id,
                    reason: err.to_string(),
                });
                self.enter_backoff(session_id, now, events);
            }
        }
    }

    fn enter_backoff(&mut self, session_id: u32, now: Instant, events: &mut Vec<SupervisorEvent>) {
        let is_crash_loop = self.crash_tracker.record(now);
        if is_crash_loop {
            events.push(SupervisorEvent::CrashLoopDetected { session_id });
        }

        let delay = backoff_delay(self.backoff_attempt);
        self.backoff_attempt = self.backoff_attempt.saturating_add(1);
        self.backoff_until = Some(now + delay);
        self.state = SupervisorState::Backoff;

        events.push(SupervisorEvent::SessionHostRestarting {
            attempt: self.backoff_attempt,
            delay,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::{MockProcessLauncher, MockSessionProvider};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn spec() -> ProcessSpec {
        ProcessSpec {
            executable: PathBuf::from("classos-session.exe"),
            args: vec![OsString::from("--dev")],
        }
    }

    #[test]
    fn backoff_delay_follows_spec_steps() {
        assert_eq!(backoff_delay(0), Duration::from_secs(1));
        assert_eq!(backoff_delay(1), Duration::from_secs(2));
        assert_eq!(backoff_delay(2), Duration::from_secs(5));
        assert_eq!(backoff_delay(3), Duration::from_secs(10));
        assert_eq!(backoff_delay(4), Duration::from_secs(30));
        assert_eq!(backoff_delay(5), Duration::from_secs(60));
        assert_eq!(backoff_delay(100), Duration::from_secs(60));
    }

    #[test]
    fn no_session_stays_idle() {
        let provider = MockSessionProvider::new(None);
        let launcher = MockProcessLauncher::new();
        let mut sup = SessionSupervisor::new(provider, launcher, spec());

        let events = sup.reconcile(Instant::now()).unwrap();
        assert!(events.is_empty());
        assert_eq!(sup.state(), SupervisorState::NoInteractiveSession);
    }

    #[test]
    fn session_appears_and_host_launches() {
        let provider = MockSessionProvider::new(Some(1));
        let launcher = MockProcessLauncher::new();
        let mut sup = SessionSupervisor::new(provider, launcher, spec());

        let now = Instant::now();
        let events = sup.reconcile(now).unwrap();

        assert!(matches!(
            sup.state(),
            SupervisorState::WaitingForIpc { session_id: 1, .. }
        ));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SupervisorEvent::SessionHostStarted { session_id: 1, .. }))
        );
    }

    #[test]
    fn ipc_ready_transitions_to_running() {
        let provider = MockSessionProvider::new(Some(1));
        let launcher = MockProcessLauncher::new();
        let mut sup = SessionSupervisor::new(provider, launcher, spec());
        let now = Instant::now();
        sup.reconcile(now).unwrap();

        let pid = match sup.state() {
            SupervisorState::WaitingForIpc { pid, .. } => pid,
            other => panic!("unexpected state {other:?}"),
        };

        sup.notify_ipc_ready(1, pid, now);
        assert_eq!(sup.state(), SupervisorState::Running { session_id: 1, pid });
    }

    #[test]
    fn crash_triggers_backoff_and_restart() {
        let provider = MockSessionProvider::new(Some(1));
        let launcher = MockProcessLauncher::new();
        let mut sup = SessionSupervisor::new(provider, launcher, spec());
        let now = Instant::now();
        sup.reconcile(now).unwrap();

        let pid = match sup.state() {
            SupervisorState::WaitingForIpc { pid, .. } => pid,
            other => panic!("unexpected state {other:?}"),
        };
        sup.notify_ipc_ready(1, pid, now);

        // Имитируем crash.
        sup.kill_for_test(pid);
        let events = sup.reconcile(now + Duration::from_secs(1)).unwrap();
        assert!(matches!(sup.state(), SupervisorState::Backoff));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SupervisorEvent::SessionHostRestarting { .. }))
        );

        // После backoff процесс должен запуститься снова.
        let events = sup.reconcile(now + Duration::from_secs(3)).unwrap();
        assert!(matches!(sup.state(), SupervisorState::WaitingForIpc { .. }));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SupervisorEvent::SessionHostStarted { .. }))
        );
    }

    #[test]
    fn ipc_loss_terminates_live_host_and_schedules_restart() {
        let provider = MockSessionProvider::new(Some(1));
        let launcher = MockProcessLauncher::new();
        let mut sup = SessionSupervisor::new(provider, launcher, spec());
        let now = Instant::now();
        sup.reconcile(now).unwrap();
        let pid = match sup.state() {
            SupervisorState::WaitingForIpc { pid, .. } => pid,
            other => panic!("unexpected state {other:?}"),
        };
        sup.notify_ipc_ready(1, pid, now);

        let events = sup.notify_ipc_lost(pid, now + Duration::from_secs(1));

        assert_eq!(sup.state(), SupervisorState::Backoff);
        assert!(!sup.launcher.is_alive(pid));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, SupervisorEvent::SessionHostRestarting { .. }))
        );
    }

    #[test]
    fn crash_loop_is_detected() {
        // Постоянная ошибка запуска тоже должна увеличивать счётчик crash-loop.
        let provider = MockSessionProvider::new(Some(1));
        let launcher = MockProcessLauncher::always_fail_launch();
        let mut sup = SessionSupervisor::new(provider, launcher, spec());

        let mut now = Instant::now();
        let mut saw_crash_loop = false;
        for attempt in 0..6u32 {
            let events = sup.reconcile(now).unwrap();
            if events
                .iter()
                .any(|e| matches!(e, SupervisorEvent::CrashLoopDetected { .. }))
            {
                saw_crash_loop = true;
            }
            // Переходим за назначенный backoff, чтобы следующий reconcile
            // действительно выполнил новую попытку.
            now += backoff_delay(attempt) + Duration::from_millis(1);
        }
        assert!(
            saw_crash_loop,
            "expected crash loop to be detected within 5 rapid failures"
        );
    }

    #[test]
    fn logout_stops_session_host() {
        let provider = MockSessionProvider::new(Some(1));
        let launcher = MockProcessLauncher::new();
        let mut sup = SessionSupervisor::new(provider, launcher, spec());
        let now = Instant::now();
        sup.reconcile(now).unwrap();
        let pid = match sup.state() {
            SupervisorState::WaitingForIpc { pid, .. } => pid,
            other => panic!("unexpected state {other:?}"),
        };
        sup.notify_ipc_ready(1, pid, now);

        sup.set_desired_for_test(None);
        let events = sup.reconcile(now + Duration::from_secs(1)).unwrap();
        assert_eq!(sup.state(), SupervisorState::NoInteractiveSession);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SupervisorEvent::SessionHostExited { .. }))
        );
    }

    #[test]
    fn user_switch_replaces_host() {
        let provider = MockSessionProvider::new(Some(1));
        let launcher = MockProcessLauncher::new();
        let mut sup = SessionSupervisor::new(provider, launcher, spec());
        let now = Instant::now();
        sup.reconcile(now).unwrap();
        let pid = match sup.state() {
            SupervisorState::WaitingForIpc { pid, .. } => pid,
            other => panic!("unexpected state {other:?}"),
        };
        sup.notify_ipc_ready(1, pid, now);

        sup.set_desired_for_test(Some(2));
        let events = sup.reconcile(now + Duration::from_secs(1)).unwrap();
        assert!(matches!(
            sup.state(),
            SupervisorState::WaitingForIpc { session_id: 2, .. }
        ));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SupervisorEvent::SessionChanged { .. }))
        );
    }

    // Тестовые helpers для mock-supervisor, не расширяющие production API.
    impl SessionSupervisor<MockSessionProvider, MockProcessLauncher> {
        fn kill_for_test(&self, pid: u32) {
            self.launcher.kill(pid);
        }

        fn set_desired_for_test(&mut self, session_id: Option<u32>) {
            self.provider.set_active(session_id);
        }
    }
}
