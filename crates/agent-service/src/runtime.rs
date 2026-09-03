//! The Tokio async runtime tying together `SessionSupervisor` (spec
//! §66-71), the real Win32-backed trait implementations, and per-Session
//! Host IPC connections (handshake + heartbeat, spec §58-65). Windows-only.
//!
//! Concurrency shape: a single event loop owns the `SessionSupervisor` and
//! reacts to three input streams merged via `tokio::select!` — a 10s
//! reconcile tick (spec §68, safety net), forwarded `ServiceEvent`s from
//! the SCM control handler (or an empty stream in dev mode), and
//! `ConnectionEvent`s reported by per-connection tasks spawned for each
//! `SessionHostStarted` supervisor event. Each connection task owns exactly
//! one Named Pipe connection end-to-end (accept -> handshake -> heartbeat
//! loop) and reports back only `Ready`/`Lost`, keeping the main loop free
//! of manual `Option<JoinHandle>` bookkeeping.

use std::time::{Duration, Instant};

use agent_core::domain::ProcessSpec;
use agent_core::supervisor::{ServiceEvent, SessionSupervisor, SupervisorEvent};
use agent_core::traits::SessionProcessLauncher;
use protocol::envelope::Payload;
use protocol::{Envelope, LOCAL_PROTOCOL_VERSION, Ping, ServiceHello};
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};
use uuid::Uuid;
use windows_service::service::{
    ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::ServiceStatusHandle;

use crate::ipc::PipeConnection;
use crate::windows_adapters::{
    LaunchMode, WindowsProcessLauncher, WindowsSessionProvider, default_session_host_path,
    user_sid_for_session,
};

const RECONCILE_INTERVAL: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(3);

type Supervisor = SessionSupervisor<WindowsSessionProvider, WindowsProcessLauncher>;

enum ConnectionEvent {
    Ready { session_id: u32, pid: u32 },
    Lost { pid: u32 },
}

fn new_message_id() -> String {
    Uuid::new_v4().to_string()
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn log_supervisor_event(event: &SupervisorEvent) {
    match event {
        SupervisorEvent::SessionDiscovered { session_id } => {
            tracing::info!(event = "SESSION_DISCOVERED", session_id)
        }
        SupervisorEvent::SessionHostStarting { session_id } => {
            tracing::info!(event = "SESSION_HOST_STARTING", session_id)
        }
        SupervisorEvent::SessionHostStarted { session_id, pid } => {
            tracing::info!(event = "SESSION_HOST_STARTED", session_id, pid)
        }
        SupervisorEvent::SessionHostLaunchFailed { session_id, reason } => {
            tracing::error!(event = "SESSION_HOST_LAUNCH_FAILED", session_id, reason)
        }
        SupervisorEvent::SessionHostExited { session_id, pid } => {
            tracing::info!(event = "SESSION_HOST_EXITED", session_id, pid)
        }
        SupervisorEvent::SessionHostRestarting { attempt, delay } => {
            tracing::info!(
                event = "SESSION_HOST_RESTARTING",
                attempt,
                delay_secs = delay.as_secs()
            )
        }
        SupervisorEvent::SessionHostStopping { session_id, pid } => {
            tracing::info!(event = "SESSION_HOST_STOPPING", session_id, pid)
        }
        SupervisorEvent::SessionChanged {
            old_session_id,
            new_session_id,
        } => tracing::info!(event = "SESSION_CHANGED", old_session_id, new_session_id),
        SupervisorEvent::NoInteractiveSession => {
            tracing::debug!(event = "NO_INTERACTIVE_SESSION")
        }
        SupervisorEvent::CrashLoopDetected { session_id } => {
            tracing::warn!(event = "SESSION_HOST_CRASH_LOOP", session_id)
        }
    }
}

/// Runs one supervisor reconciliation pass, logs the resulting events, and
/// spawns a connection task for any freshly-started Session Host.
fn do_reconcile(
    supervisor: &mut Supervisor,
    service_instance_id: &str,
    conn_tx: &mpsc::UnboundedSender<ConnectionEvent>,
) {
    let events = match supervisor.reconcile(Instant::now()) {
        Ok(events) => events,
        Err(err) => {
            tracing::error!(error = %err, "supervisor reconcile failed");
            return;
        }
    };

    for event in &events {
        log_supervisor_event(event);
    }

    for event in events {
        if let SupervisorEvent::SessionHostStarted { session_id, pid } = event {
            let Some(pipe_name) = supervisor.launcher().pipe_name_for(pid) else {
                tracing::error!(session_id, pid, "no pipe name recorded for launched pid");
                continue;
            };
            let user_sid = match user_sid_for_session(session_id) {
                Ok(sid) => sid,
                Err(err) => {
                    tracing::error!(session_id, error = %err, "failed to resolve session user SID");
                    continue;
                }
            };
            let conn_tx = conn_tx.clone();
            let service_instance_id = service_instance_id.to_string();
            tokio::spawn(connection_task(
                pipe_name,
                user_sid,
                session_id,
                pid,
                service_instance_id,
                conn_tx,
            ));
        }
    }
}

/// Owns one Named Pipe connection end-to-end: accept, handshake, then the
/// heartbeat loop, per spec §58-65. Reports back only `Ready`/`Lost`.
async fn connection_task(
    pipe_name: String,
    user_sid: String,
    session_id: u32,
    pid: u32,
    service_instance_id: String,
    events_tx: mpsc::UnboundedSender<ConnectionEvent>,
) {
    let mut connection = match PipeConnection::accept_one(&pipe_name, &user_sid).await {
        Ok(connection) => connection,
        Err(err) => {
            tracing::warn!(session_id, pid, error = %err, event = "IPC_HANDSHAKE_FAILED");
            let _ = events_tx.send(ConnectionEvent::Lost { pid });
            return;
        }
    };

    // Never trust the client's own claims about its identity: the pipe
    // accept path already independently resolved peer_session_id/peer_pid
    // via ProcessIdToSessionId/GetNamedPipeClientProcessId (spec §59-60,
    // §132). Reject if either doesn't match what this launch was for —
    // the pid check in particular guards against a stale/unrelated process
    // somehow connecting to a freshly (re)created pipe of the same name.
    if connection.peer_session_id() != session_id || connection.peer_pid() != pid {
        tracing::warn!(
            session_id,
            pid,
            observed_session_id = connection.peer_session_id(),
            observed_pid = connection.peer_pid(),
            event = "IPC_HANDSHAKE_FAILED",
            reason = "peer identity mismatch"
        );
        let _ = events_tx.send(ConnectionEvent::Lost { pid });
        return;
    }

    let hello = match connection.recv().await {
        Ok(Some(envelope)) => envelope,
        _ => {
            tracing::warn!(
                session_id,
                pid,
                event = "IPC_HANDSHAKE_FAILED",
                reason = "no hello"
            );
            let _ = events_tx.send(ConnectionEvent::Lost { pid });
            return;
        }
    };

    let protocol_version_ok = matches!(
        &hello.payload,
        Some(Payload::SessionHello(hello)) if hello.protocol_version == LOCAL_PROTOCOL_VERSION
    );
    if !protocol_version_ok {
        tracing::warn!(
            session_id,
            pid,
            event = "IPC_HANDSHAKE_FAILED",
            reason = "missing/mismatched SessionHello"
        );
        let _ = events_tx.send(ConnectionEvent::Lost { pid });
        return;
    }

    let service_hello = Envelope {
        message_id: new_message_id(),
        payload: Some(Payload::ServiceHello(ServiceHello {
            protocol_version: LOCAL_PROTOCOL_VERSION,
            service_instance_id,
        })),
    };
    if connection.send(&service_hello).await.is_err() {
        let _ = events_tx.send(ConnectionEvent::Lost { pid });
        return;
    }

    tracing::info!(session_id, pid, event = "IPC_HANDSHAKE_OK");
    let _ = events_tx.send(ConnectionEvent::Ready { session_id, pid });

    let mut ticker = interval(HEARTBEAT_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut sequence: u64 = 0;
    let mut last_pong = Instant::now();

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if last_pong.elapsed() > HEARTBEAT_TIMEOUT {
                    tracing::warn!(session_id, pid, event = "IPC_HEARTBEAT_TIMEOUT");
                    break;
                }
                sequence += 1;
                let ping = Envelope {
                    message_id: new_message_id(),
                    payload: Some(Payload::Ping(Ping {
                        sequence,
                        sent_at_unix_ms: now_unix_ms(),
                    })),
                };
                if connection.send(&ping).await.is_err() {
                    break;
                }
            }
            recv = connection.recv() => {
                match recv {
                    Ok(Some(envelope)) => {
                        if matches!(envelope.payload, Some(Payload::Pong(_))) {
                            last_pong = Instant::now();
                        }
                        // Unknown/other message types are logged and
                        // otherwise ignored (spec §127: never panic on an
                        // unexpected message).
                    }
                    Ok(None) => {
                        tracing::info!(session_id, pid, event = "SESSION_HOST_DISCONNECTED");
                        break;
                    }
                    Err(err) => {
                        tracing::warn!(session_id, pid, error = %err, event = "SESSION_HOST_DISCONNECTED");
                        break;
                    }
                }
            }
        }
    }

    let _ = events_tx.send(ConnectionEvent::Lost { pid });
}

/// Builds the supervisor and runs the main event loop until a `Stop`/
/// `Shutdown` `ServiceEvent` is observed or `service_events` closes.
///
/// `status_handle` is `Some` when running as a real Windows Service
/// (spec §16-22): it lets `graceful_shutdown` report `StopPending` with
/// incrementing checkpoints to the SCM while shutdown work is in
/// progress (spec §17 — all four states, including `STOP_PENDING`, are
/// mandatory; reporting only `Running` and then jumping straight to
/// `Stopped` risks the SCM/`Stop-Service` treating the service as
/// unresponsive). It is `None` in dev-mode `run` (spec §12-13), which has
/// no SCM to report to.
pub async fn run(
    mode: LaunchMode,
    mut service_events: mpsc::UnboundedReceiver<ServiceEvent>,
    status_handle: Option<ServiceStatusHandle>,
) {
    let session_host_path = match default_session_host_path() {
        Ok(path) => path,
        Err(err) => {
            tracing::error!(error = %err, "cannot resolve classos-session.exe path");
            return;
        }
    };

    let provider = WindowsSessionProvider;
    let launcher = WindowsProcessLauncher::new(session_host_path.clone(), mode);
    // The ProcessSpec's own executable/args are unused by
    // WindowsProcessLauncher (it builds --session-id/--pipe dynamically
    // per launch, see windows_adapters.rs); it exists to satisfy
    // SessionSupervisor's OS-independent constructor signature, which
    // mocks in agent-core's tests use meaningfully.
    let spec = ProcessSpec {
        executable: session_host_path,
        args: Vec::new(),
    };

    let mut supervisor: Supervisor = SessionSupervisor::new(provider, launcher, spec);
    let service_instance_id = agent_core::config::new_service_instance_id().to_string();

    let mut reconcile_tick = interval(RECONCILE_INTERVAL);
    reconcile_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let (conn_tx, mut conn_rx) = mpsc::unbounded_channel::<ConnectionEvent>();

    loop {
        tokio::select! {
            _ = reconcile_tick.tick() => {
                do_reconcile(&mut supervisor, &service_instance_id, &conn_tx);
            }
            maybe_event = service_events.recv() => {
                match maybe_event {
                    Some(ServiceEvent::Stop) | Some(ServiceEvent::Shutdown) => {
                        graceful_shutdown(&mut supervisor, status_handle).await;
                        return;
                    }
                    Some(_other) => {
                        do_reconcile(&mut supervisor, &service_instance_id, &conn_tx);
                    }
                    None => {
                        // Event source closed (e.g. dev mode with no SCM):
                        // keep running on the reconcile tick alone.
                    }
                }
            }
            maybe_conn_event = conn_rx.recv() => {
                match maybe_conn_event {
                    Some(ConnectionEvent::Ready { session_id, pid }) => {
                        supervisor.notify_ipc_ready(session_id, pid, Instant::now());
                    }
                    Some(ConnectionEvent::Lost { pid }) => {
                        // The next reconcile's is_alive() check will notice
                        // an actual crash and drive backoff/restart; this
                        // event is purely informational logging here
                        // (spec §68-69: reconciliation, not per-event
                        // branching, is the source of truth for lifecycle
                        // decisions).
                        tracing::debug!(pid, "connection task reported lost");
                    }
                    None => {}
                }
            }
        }
    }
}

/// Interval between `StopPending` checkpoint updates while shutdown work
/// is in progress. Must be comfortably shorter than the `wait_hint` given
/// to each update, per Win32's service-control-manager contract: the SCM
/// treats a service as hung only if it fails to report a fresh checkpoint
/// within the `wait_hint` of the previous one, not within the eventual
/// total shutdown time.
const STOP_PENDING_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(1);

fn report_stop_pending(status_handle: Option<ServiceStatusHandle>, checkpoint: u32) {
    let Some(status_handle) = status_handle else {
        return;
    };
    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StopPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint,
        wait_hint: STOP_PENDING_CHECKPOINT_INTERVAL * 3,
        process_id: None,
    });
}

async fn graceful_shutdown(
    supervisor: &mut Supervisor,
    status_handle: Option<ServiceStatusHandle>,
) {
    tracing::info!(event = "SERVICE_STOPPING");

    // Report StopPending immediately (spec §17: START_PENDING/RUNNING/
    // STOP_PENDING/STOPPED are all mandatory) so the SCM — and tools like
    // `Stop-Service` built on top of it — see a state transition right
    // away instead of "Running" persisting until the process disappears.
    let mut checkpoint = 1;
    report_stop_pending(status_handle, checkpoint);

    // T0's SessionSupervisor doesn't expose a dedicated "shut down and
    // wait" API; reusing reconcile() with a provider that (from the real
    // WTS API's point of view) may still report an active session doesn't
    // stop the host. Terminate directly via the launcher for whatever PID
    // is currently tracked, then let the deadline pass.
    if let agent_core::supervisor::SupervisorState::Running { pid, .. }
    | agent_core::supervisor::SupervisorState::WaitingForIpc { pid, .. } = supervisor.state()
    {
        let _ = supervisor.launcher().terminate(pid);
    }

    let mut remaining = SHUTDOWN_GRACE_PERIOD;
    while !remaining.is_zero() {
        let step = remaining.min(STOP_PENDING_CHECKPOINT_INTERVAL);
        tokio::time::sleep(step).await;
        remaining -= step;
        checkpoint += 1;
        report_stop_pending(status_handle, checkpoint);
    }
}
