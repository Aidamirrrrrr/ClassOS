//! [`SessionSupervisor`]: the core T0 desired-state reconciliation loop
//! (spec §23, §66-71). Written entirely against [`crate::traits`] so it is
//! testable on any host with [`crate::mocks`], with zero Win32 dependency.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::domain::ProcessSpec;
use crate::error::Result;
use crate::traits::{SessionProcessLauncher, SessionProvider};

/// Internal events a real Windows Service forwards from its SCM control
/// handler (spec §21). The supervisor itself does not branch on these
/// directly (desired-state model, spec §67) — they exist so the runtime
/// can trigger an out-of-band reconcile promptly instead of waiting for the
/// next periodic tick, and so lock/unlock state can be relayed elsewhere
/// (e.g. into `SessionInfo`).
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

/// Supervisor state machine states (spec §24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorState {
    NoInteractiveSession,
    Starting { session_id: u32 },
    WaitingForIpc { session_id: u32, pid: u32 },
    Running { session_id: u32, pid: u32 },
    Stopping { session_id: u32, pid: u32 },
    Backoff,
}

/// Observable outcomes of a single [`SessionSupervisor::reconcile`] call.
/// Used for structured logging (spec §82-83) and for asserting behaviour in
/// unit tests without inspecting private state.
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

/// Exponential backoff steps in seconds (spec §70): 1s, 2s, 5s, 10s, 30s,
/// capped at 60s.
pub const BACKOFF_STEPS_SECS: [u64; 6] = [1, 2, 5, 10, 30, 60];

/// A restart is considered "stable" (resetting backoff) after running this
/// long without crashing (spec §70).
pub const STABLE_RUN_DURATION: Duration = Duration::from_secs(60);

/// Crash-loop detection window and threshold (spec §71).
pub const CRASH_LOOP_WINDOW: Duration = Duration::from_secs(120);
pub const CRASH_LOOP_THRESHOLD: usize = 5;

/// How long the supervisor waits in `WaitingForIpc` before treating the
/// launch as failed (not specified verbatim in T0, chosen to match the
/// "<10s ready" target in spec §167 with margin for slow desktop startup,
/// spec §168).
pub const WAITING_FOR_IPC_TIMEOUT: Duration = Duration::from_secs(30);

/// Pure function computing the backoff delay for a given zero-based retry
/// attempt (spec §70, §98 "backoff calculation" is a required unit test).
pub fn backoff_delay(attempt: u32) -> Duration {
    let idx = (attempt as usize).min(BACKOFF_STEPS_SECS.len() - 1);
    Duration::from_secs(BACKOFF_STEPS_SECS[idx])
}

/// Tracks recent crash timestamps within a sliding window to detect crash
/// loops (spec §71).
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

/// The T0 session supervisor: computes desired state from
/// [`SessionProvider`], compares to actual managed process state, and
/// repairs drift via [`SessionProcessLauncher`] (spec §66-71).
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

    /// Read-only access to the launcher, so the IPC wiring layer can look
    /// up launch-specific out-of-band data (e.g. the Named Pipe name
    /// generated for a given pid) without the supervisor itself needing to
    /// know about IPC transport concerns.
    pub fn launcher(&self) -> &L {
        &self.launcher
    }

    /// Called by the IPC layer once handshake completes for a connection
    /// bound to `session_id`/`pid` (independently verified — spec §59-60).
    /// Transitions `WaitingForIpc` -> `Running` if it matches the currently
    /// tracked launch.
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

    /// Runs one reconciliation pass: determine desired state from the
    /// session provider, compare to actual tracked state, repair drift.
    /// Safe to call at any time, including redundantly (spec §68-69).
    pub fn reconcile(&mut self, now: Instant) -> Result<Vec<SupervisorEvent>> {
        let mut events = Vec::new();
        let desired = self.provider.active_console_session()?;

        // Crash-storm-safe: never restart before backoff_until elapses.
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
                // Starting is transient within a single reconcile call; if
                // observed here it means a previous attempt did not
                // complete synchronously. Treat as NoInteractiveSession and
                // retry on the next pass.
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
                // Stopping is transient in this synchronous model; ensure
                // termination completed and settle.
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
                // Logout: desired state is no session host at all.
                events.push(SupervisorEvent::SessionHostStopping { session_id, pid });
                let _ = self.launcher.terminate(pid);
                events.push(SupervisorEvent::SessionHostExited { session_id, pid });
                self.state = SupervisorState::NoInteractiveSession;
                self.running_since = None;
                self.waiting_since = None;
            }
            Some(session) if session.session_id != session_id => {
                // Active console session changed: replace the managed host.
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
                // Crashed.
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
                // else: still legitimately waiting, nothing to do.
            }
            Some(_) => {
                // Running and healthy: reset backoff once stable.
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

        // Simulate crash.
        sup.kill_for_test(pid);
        let events = sup.reconcile(now + Duration::from_secs(1)).unwrap();
        assert!(matches!(sup.state(), SupervisorState::Backoff));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SupervisorEvent::SessionHostRestarting { .. }))
        );

        // After backoff elapses, it should relaunch.
        let events = sup.reconcile(now + Duration::from_secs(3)).unwrap();
        assert!(matches!(sup.state(), SupervisorState::WaitingForIpc { .. }));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SupervisorEvent::SessionHostStarted { .. }))
        );
    }

    #[test]
    fn crash_loop_is_detected() {
        // A launcher that always fails to launch also drives the
        // crash-loop counter (spec §71 applies to repeated failed restart
        // attempts, not only post-launch crashes): each reconcile call
        // that hits an unexpired-backoff-cleared retry counts as one
        // failure.
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
            // Advance past whatever backoff was just scheduled so the next
            // reconcile actually attempts a relaunch instead of being
            // suppressed by an unexpired backoff window. `attempt` tracks
            // the supervisor's internal backoff_attempt in lockstep since
            // both start at 0 and increment once per failed attempt.
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

    // Test-only helpers layered onto the concrete mock-backed supervisor,
    // to avoid growing the production API surface with test-specific
    // methods.
    impl SessionSupervisor<MockSessionProvider, MockProcessLauncher> {
        fn kill_for_test(&self, pid: u32) {
            self.launcher.kill(pid);
        }

        fn set_desired_for_test(&mut self, session_id: Option<u32>) {
            self.provider.set_active(session_id);
        }
    }
}
