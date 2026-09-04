//! Tokio runtime связывает `SessionSupervisor`, реализации поверх Win32 и
//! отдельные IPC-соединения Session Host. Только Windows.
//!
//! Один event loop владеет supervisor и через `tokio::select!` принимает
//! периодический reconcile, события SCM и события соединений. Каждая задача
//! соединения целиком владеет одним Named Pipe и сообщает только Ready/Lost.

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use agent_core::commands::{
    CommandAdmission, CommandDeduplicator, catalog_application, is_allowed_url,
};
use agent_core::domain::ProcessSpec;
use agent_core::supervisor::{ServiceEvent, SessionSupervisor, SupervisorEvent};
use agent_core::traits::SessionProcessLauncher;
use protocol::envelope::Payload;
use protocol::{
    Envelope, GetSessionInfo, LOCAL_PROTOCOL_VERSION, Ping, ServiceHello, SessionCommand,
    SessionCommandResult, Shutdown,
};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;
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
use windows_platform::power::{PowerAction, execute_power_action};

const RECONCILE_INTERVAL: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(3);

type Supervisor = SessionSupervisor<WindowsSessionProvider, WindowsProcessLauncher>;

enum ConnectionEvent {
    Ready { session_id: u32, pid: u32 },
    Lost { pid: u32 },
}

enum ConnectionCommand {
    Shutdown,
    Capture {
        request: protocol::CaptureRequest,
        response: oneshot::Sender<Result<protocol::Frame, protocol::CaptureError>>,
    },
    RemoteStart {
        session_id: String,
        response: oneshot::Sender<Result<(), protocol::CaptureError>>,
    },
    RemoteStop {
        reason: String,
    },
    RemoteInput {
        event: protocol::RemoteInputEvent,
    },
    SessionCommand {
        command: SessionCommand,
        response: oneshot::Sender<Result<SessionCommandResult, protocol::CaptureError>>,
    },
}

#[derive(Default)]
struct CaptureBroker {
    current: std::sync::Mutex<Option<(u32, mpsc::UnboundedSender<ConnectionCommand>)>>,
    remote_owner: std::sync::Mutex<agent_core::remote::RemoteControlSession>,
    command_cache: std::sync::Mutex<CommandDeduplicator>,
}

impl CaptureBroker {
    fn claim_remote(
        &self,
        owner: String,
        session_id: String,
    ) -> Result<(), agent_core::remote::RemoteControlError> {
        self.remote_owner
            .lock()
            .expect("remote owner mutex poisoned")
            .start(owner, session_id)
    }
    fn release_remote(&self, owner: &str) -> Option<String> {
        self.remote_owner
            .lock()
            .expect("remote owner mutex poisoned")
            .stop(owner)
            .ok()
    }
    fn accepts_remote_input(&self, owner: &str) -> bool {
        self.remote_owner
            .lock()
            .expect("remote owner mutex poisoned")
            .accepts_input(owner)
    }
    fn set(&self, pid: u32, sender: mpsc::UnboundedSender<ConnectionCommand>) {
        *self.current.lock().expect("capture broker mutex poisoned") = Some((pid, sender));
    }

    /// Потеря текущего Session Host немедленно отзывает его control-сессию.
    fn clear(&self, pid: u32) -> Option<String> {
        let mut current = self.current.lock().expect("capture broker mutex poisoned");
        if current
            .as_ref()
            .is_some_and(|(current_pid, _)| *current_pid == pid)
        {
            *current = None;
            return self
                .remote_owner
                .lock()
                .expect("remote owner mutex poisoned")
                .disconnect();
        }
        None
    }

    async fn capture(
        &self,
        request: protocol::CaptureRequest,
    ) -> Result<protocol::Frame, protocol::CaptureError> {
        let sender = self
            .current
            .lock()
            .expect("capture broker mutex poisoned")
            .as_ref()
            .map(|(_, sender)| sender.clone())
            .ok_or_else(|| protocol::CaptureError {
                code: "NO_INTERACTIVE_SESSION".to_owned(),
                message: "нет активной пользовательской сессии".to_owned(),
            })?;
        let (response, receiver) = oneshot::channel();
        sender
            .send(ConnectionCommand::Capture { request, response })
            .map_err(|_| protocol::CaptureError {
                code: "SESSION_HOST_DISCONNECTED".to_owned(),
                message: "Session Host недоступен".to_owned(),
            })?;
        tokio::time::timeout(Duration::from_secs(3), receiver)
            .await
            .map_err(|_| protocol::CaptureError {
                code: "CAPTURE_TIMEOUT".to_owned(),
                message: "время ожидания снимка истекло".to_owned(),
            })?
            .map_err(|_| protocol::CaptureError {
                code: "SESSION_HOST_DISCONNECTED".to_owned(),
                message: "Session Host закрыл запрос".to_owned(),
            })?
    }

    async fn start_remote(&self, session_id: String) -> Result<(), protocol::CaptureError> {
        let sender = self
            .current
            .lock()
            .expect("capture broker mutex poisoned")
            .as_ref()
            .map(|(_, sender)| sender.clone())
            .ok_or_else(|| protocol::CaptureError {
                code: "NO_INTERACTIVE_SESSION".to_owned(),
                message: "нет активной пользовательской сессии".to_owned(),
            })?;
        let (response, receiver) = oneshot::channel();
        sender
            .send(ConnectionCommand::RemoteStart {
                session_id,
                response,
            })
            .map_err(|_| protocol::CaptureError {
                code: "SESSION_HOST_DISCONNECTED".to_owned(),
                message: "Session Host недоступен".to_owned(),
            })?;
        tokio::time::timeout(Duration::from_secs(3), receiver)
            .await
            .map_err(|_| protocol::CaptureError {
                code: "REMOTE_CONTROL_TIMEOUT".to_owned(),
                message: "Session Host не подтвердил control-сессию".to_owned(),
            })?
            .map_err(|_| protocol::CaptureError {
                code: "SESSION_HOST_DISCONNECTED".to_owned(),
                message: "Session Host закрыл control-сессию".to_owned(),
            })?
    }

    fn stop_remote(&self, reason: &str) {
        if let Some((_, sender)) = self
            .current
            .lock()
            .expect("capture broker mutex poisoned")
            .as_ref()
        {
            let _ = sender.send(ConnectionCommand::RemoteStop {
                reason: reason.to_owned(),
            });
        }
    }

    fn send_remote_input(
        &self,
        event: protocol::RemoteInputEvent,
    ) -> Result<(), protocol::CaptureError> {
        let sender = self
            .current
            .lock()
            .expect("capture broker mutex poisoned")
            .as_ref()
            .map(|(_, sender)| sender.clone())
            .ok_or_else(|| protocol::CaptureError {
                code: "NO_INTERACTIVE_SESSION".to_owned(),
                message: "нет активной пользовательской сессии".to_owned(),
            })?;
        sender
            .send(ConnectionCommand::RemoteInput { event })
            .map_err(|_| protocol::CaptureError {
                code: "SESSION_HOST_DISCONNECTED".to_owned(),
                message: "Session Host недоступен".to_owned(),
            })
    }

    fn admit_command(&self, command_id: &str, expires_at_unix_ms: i64) -> CommandAdmission {
        self.command_cache
            .lock()
            .expect("command cache mutex poisoned")
            .admit(command_id, expires_at_unix_ms, now_unix_ms())
    }

    fn complete_command(&self, command_id: &str, result: protocol::network::CommandResult) {
        self.command_cache
            .lock()
            .expect("command cache mutex poisoned")
            .complete(command_id, result, now_unix_ms());
    }

    async fn execute_session_command(
        &self,
        command: SessionCommand,
    ) -> Result<SessionCommandResult, protocol::CaptureError> {
        let sender = self
            .current
            .lock()
            .expect("capture broker mutex poisoned")
            .as_ref()
            .map(|(_, sender)| sender.clone())
            .ok_or_else(|| protocol::CaptureError {
                code: "NO_INTERACTIVE_SESSION".to_owned(),
                message: "нет активной пользовательской сессии".to_owned(),
            })?;
        let (response, receiver) = oneshot::channel();
        sender
            .send(ConnectionCommand::SessionCommand { command, response })
            .map_err(|_| protocol::CaptureError {
                code: "SESSION_HOST_DISCONNECTED".to_owned(),
                message: "Session Host недоступен".to_owned(),
            })?;
        tokio::time::timeout(Duration::from_secs(3), receiver)
            .await
            .map_err(|_| protocol::CaptureError {
                code: "COMMAND_TIMEOUT".to_owned(),
                message: "Session Host не подтвердил команду".to_owned(),
            })?
            .map_err(|_| protocol::CaptureError {
                code: "SESSION_HOST_DISCONNECTED".to_owned(),
                message: "Session Host закрыл команду".to_owned(),
            })?
    }
}

fn to_local_remote_input(
    event: protocol::network::RemoteInputEvent,
) -> Option<protocol::RemoteInputEvent> {
    use protocol::network::remote_input_event::Event as NetworkEvent;
    use protocol::remote_input_event::Event as LocalEvent;
    let event = match event.event? {
        NetworkEvent::MouseMove(value) => LocalEvent::MouseMove(protocol::MouseMove {
            x: value.x,
            y: value.y,
        }),
        NetworkEvent::MouseButton(value) => LocalEvent::MouseButton(protocol::MouseButton {
            button: value.button,
            is_down: value.is_down,
            x: value.x,
            y: value.y,
        }),
        NetworkEvent::MouseWheel(value) => {
            LocalEvent::MouseWheel(protocol::MouseWheel { delta: value.delta })
        }
        NetworkEvent::KeyEvent(value) => LocalEvent::KeyEvent(protocol::KeyEvent {
            virtual_key_code: value.virtual_key_code,
            is_down: value.is_down,
        }),
    };
    Some(protocol::RemoteInputEvent { event: Some(event) })
}

fn command_result(
    command_id: String,
    success: bool,
    error_code: impl Into<String>,
    message: impl Into<String>,
) -> protocol::network::CommandResult {
    protocol::network::CommandResult {
        command_id,
        success,
        error_code: error_code.into(),
        message: message.into(),
    }
}

async fn execute_classroom_command(
    command: protocol::network::Command,
    capture_broker: &CaptureBroker,
) -> protocol::network::CommandResult {
    use protocol::network::command::Body as NetworkBody;
    use protocol::session_command::Body as SessionBody;

    let command_id = command.command_id.clone();
    let session_body = match command.body {
        Some(NetworkBody::LockDevice(_)) => Some(SessionBody::LockScreen(protocol::LockScreen {})),
        Some(NetworkBody::UnlockDevice(_)) => {
            Some(SessionBody::UnlockScreen(protocol::UnlockScreen {}))
        }
        Some(NetworkBody::ShowMessage(message))
            if !message.text.is_empty() && message.text.len() <= 1_000 =>
        {
            Some(SessionBody::ShowMessage(protocol::ShowMessage {
                text: message.text,
            }))
        }
        Some(NetworkBody::ShowMessage(_)) => {
            return command_result(
                command_id,
                false,
                "MESSAGE_INVALID",
                "текст сообщения должен содержать от 1 до 1000 байт",
            );
        }
        Some(NetworkBody::LaunchApplication(application))
            if catalog_application(&application.application_id).is_some() =>
        {
            Some(SessionBody::LaunchApplication(
                protocol::LaunchApplication {
                    application_id: application.application_id,
                },
            ))
        }
        Some(NetworkBody::LaunchApplication(_)) => {
            return command_result(
                command_id,
                false,
                "APPLICATION_NOT_ALLOWED",
                "приложение отсутствует в локальном каталоге",
            );
        }
        Some(NetworkBody::OpenUrl(open_url)) if is_allowed_url(&open_url.url) => {
            Some(SessionBody::OpenUrl(protocol::OpenUrl {
                url: open_url.url,
            }))
        }
        Some(NetworkBody::OpenUrl(_)) => {
            return command_result(
                command_id,
                false,
                "URL_NOT_ALLOWED",
                "разрешены только HTTP и HTTPS URL без пробелов",
            );
        }
        Some(NetworkBody::RestartDevice(_)) => {
            return match execute_power_action(PowerAction::Restart) {
                Ok(()) => command_result(command_id, true, "", "перезагрузка запущена"),
                Err(error) => {
                    command_result(command_id, false, "RESTART_FAILED", error.to_string())
                }
            };
        }
        Some(NetworkBody::ShutdownDevice(_)) => {
            return match execute_power_action(PowerAction::Shutdown) {
                Ok(()) => command_result(command_id, true, "", "выключение запущено"),
                Err(error) => {
                    command_result(command_id, false, "SHUTDOWN_FAILED", error.to_string())
                }
            };
        }
        // Policy-команды T6 исполняет сам Service: они привилегированные и не
        // имеют маршрута в пользовательскую сессию. До подключения Windows
        // enforcement отвечаем явным отказом, а не молчаливым успехом.
        Some(
            NetworkBody::ApplyPolicy(_)
            | NetworkBody::RollbackPolicy(_)
            | NetworkBody::FocusModeEnable(_)
            | NetworkBody::FocusModeDisable(_),
        ) => {
            return command_result(
                command_id,
                false,
                "POLICY_NOT_CONFIGURED",
                "policy enforcement ещё не подключён на этом устройстве",
            );
        }
        None => {
            return command_result(
                command_id,
                false,
                "COMMAND_UNSUPPORTED",
                "отсутствует действие команды",
            );
        }
    };

    let local = SessionCommand {
        command_id: command_id.clone(),
        body: session_body,
    };
    match capture_broker.execute_session_command(local).await {
        Ok(result) => command_result(
            command_id,
            result.success,
            result.error_code,
            result.message,
        ),
        Err(error) => command_result(command_id, false, error.code, error.message),
    }
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

/// Выполняет один reconcile, журналирует события и создаёт задачу IPC для
/// нового Session Host.
fn do_reconcile(
    supervisor: &mut Supervisor,
    service_instance_id: &str,
    conn_tx: &mpsc::UnboundedSender<ConnectionEvent>,
    connections: &mut HashMap<u32, mpsc::UnboundedSender<ConnectionCommand>>,
    capture_broker: &Arc<CaptureBroker>,
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
            let (command_tx, command_rx) = mpsc::unbounded_channel();
            connections.insert(pid, command_tx.clone());
            capture_broker.set(pid, command_tx);
            tokio::spawn(connection_task(
                pipe_name,
                user_sid,
                session_id,
                pid,
                service_instance_id,
                conn_tx,
                command_rx,
            ));
        }
    }
}

/// Полностью обслуживает одно соединение: accept, handshake и heartbeat.
async fn connection_task(
    pipe_name: String,
    user_sid: String,
    session_id: u32,
    pid: u32,
    service_instance_id: String,
    events_tx: mpsc::UnboundedSender<ConnectionEvent>,
    mut commands_rx: mpsc::UnboundedReceiver<ConnectionCommand>,
) {
    tracing::info!(session_id, pid, pipe_name, event = "IPC_LISTENING");
    let mut connection = match PipeConnection::accept_one(&pipe_name, &user_sid).await {
        Ok(connection) => connection,
        Err(err) => {
            tracing::warn!(session_id, pid, error = %err, event = "IPC_HANDSHAKE_FAILED");
            let _ = events_tx.send(ConnectionEvent::Lost { pid });
            return;
        }
    };
    tracing::info!(session_id, pid, event = "IPC_CONNECTED");

    // Не доверяем идентификаторам клиента: PID и session id уже независимо
    // получены через WinAPI. Несовпадение означает посторонний или устаревший
    // процесс и приводит к отказу в handshake.
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
    tracing::info!(session_id, pid, event = "SESSION_HOST_CONNECTED");
    let _ = events_tx.send(ConnectionEvent::Ready { session_id, pid });

    // Сразу проверяем обязательный обмен GetSessionInfo/SessionInfo.
    let get_session_info = Envelope {
        message_id: new_message_id(),
        payload: Some(Payload::GetSessionInfo(GetSessionInfo {})),
    };
    if connection.send(&get_session_info).await.is_err() {
        let _ = events_tx.send(ConnectionEvent::Lost { pid });
        return;
    }

    let mut ticker = interval(HEARTBEAT_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut sequence: u64 = 0;
    let mut last_pong = Instant::now();
    let mut pending_capture: Option<
        oneshot::Sender<Result<protocol::Frame, protocol::CaptureError>>,
    > = None;
    let mut pending_remote_start: Option<oneshot::Sender<Result<(), protocol::CaptureError>>> =
        None;
    let mut pending_session_command: Option<
        oneshot::Sender<Result<SessionCommandResult, protocol::CaptureError>>,
    > = None;

    loop {
        tokio::select! {
            command = commands_rx.recv() => {
                match command {
                    Some(ConnectionCommand::Shutdown) => {
                        let shutdown = Envelope {
                            message_id: new_message_id(),
                            payload: Some(Payload::Shutdown(Shutdown {})),
                        };
                        let _ = connection.send(&shutdown).await;
                        tracing::info!(session_id, pid, event = "IPC_SHUTDOWN_SENT");
                        break;
                    }
                    Some(ConnectionCommand::Capture { request, response }) => {
                        if pending_capture.is_some() {
                            let _ = response.send(Err(protocol::CaptureError { code: "CAPTURE_BUSY".to_owned(), message: "предыдущий снимок ещё обрабатывается".to_owned() }));
                            continue;
                        }
                        let request = Envelope {
                            message_id: new_message_id(),
                            payload: Some(Payload::CaptureRequest(request)),
                        };
                        if connection.send(&request).await.is_err() {
                            let _ = response.send(Err(protocol::CaptureError { code: "SESSION_HOST_DISCONNECTED".to_owned(), message: "не удалось отправить запрос Session Host".to_owned() }));
                            break;
                        }
                        pending_capture = Some(response);
                    }
                    Some(ConnectionCommand::RemoteStart { session_id, response }) => {
                        if pending_remote_start.is_some() {
                            let _ = response.send(Err(protocol::CaptureError { code: "REMOTE_CONTROL_BUSY".to_owned(), message: "предыдущий запрос control ещё обрабатывается".to_owned() }));
                            continue;
                        }
                        let request = Envelope { message_id: new_message_id(), payload: Some(Payload::RemoteControlStart(protocol::RemoteControlStart { session_id })) };
                        if connection.send(&request).await.is_err() {
                            let _ = response.send(Err(protocol::CaptureError { code: "SESSION_HOST_DISCONNECTED".to_owned(), message: "не удалось начать control-сессию".to_owned() }));
                            break;
                        }
                        pending_remote_start = Some(response);
                    }
                    Some(ConnectionCommand::RemoteStop { reason }) => {
                        let request = Envelope { message_id: new_message_id(), payload: Some(Payload::RemoteControlStop(protocol::RemoteControlStop { reason })) };
                        if connection.send(&request).await.is_err() { break; }
                    }
                    Some(ConnectionCommand::RemoteInput { event }) => {
                        let request = Envelope { message_id: new_message_id(), payload: Some(Payload::RemoteInputEvent(event)) };
                        if connection.send(&request).await.is_err() { break; }
                    }
                    Some(ConnectionCommand::SessionCommand { command, response }) => {
                        if pending_session_command.is_some() {
                            let _ = response.send(Err(protocol::CaptureError { code: "COMMAND_BUSY".to_owned(), message: "предыдущая команда ещё обрабатывается".to_owned() }));
                            continue;
                        }
                        let request = Envelope { message_id: new_message_id(), payload: Some(Payload::SessionCommand(command)) };
                        if connection.send(&request).await.is_err() {
                            let _ = response.send(Err(protocol::CaptureError { code: "SESSION_HOST_DISCONNECTED".to_owned(), message: "не удалось отправить команду Session Host".to_owned() }));
                            break;
                        }
                        pending_session_command = Some(response);
                    }
                    None => break,
                }
            }
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
                        match envelope.payload {
                            Some(Payload::Pong(_)) => last_pong = Instant::now(),
                            Some(Payload::SessionInfo(info)) => {
                                tracing::info!(
                                    session_id = info.session_id,
                                    pid = info.pid,
                                    username = info.username,
                                    reported_locked = info.is_locked,
                                    event = "SESSION_INFO_RECEIVED"
                                );
                            }
                            Some(Payload::Frame(frame)) => {
                                if let Some(response) = pending_capture.take() { let _ = response.send(Ok(frame.clone())); }
                                tracing::info!(session_id, pid, display_id = frame.display_id, width = frame.width, height = frame.height, format = frame.format, event = "CAPTURE_FRAME_RECEIVED");
                            }
                            Some(Payload::CaptureError(error)) => {
                                if let Some(response) = pending_capture.take() { let _ = response.send(Err(error.clone())); }
                                tracing::warn!(session_id, pid, code = error.code, message = error.message, event = "CAPTURE_FAILED");
                            }
                            Some(Payload::RemoteControlStarted(_)) => {
                                if let Some(response) = pending_remote_start.take() { let _ = response.send(Ok(())); }
                                tracing::info!(session_id, pid, event = "REMOTE_CONTROL_SESSION_READY");
                            }
                            Some(Payload::RemoteControlStopped(stopped)) => {
                                tracing::info!(session_id, pid, reason = stopped.reason, event = "REMOTE_CONTROL_SESSION_STOPPED");
                            }
                            Some(Payload::SessionCommandResult(result)) => {
                                if let Some(response) = pending_session_command.take() {
                                    let _ = response.send(Ok(result));
                                }
                            }
                            _ => tracing::warn!(session_id, pid, "received unexpected IPC message"),
                        }
                        // Неожиданные сообщения журналируются без panic.
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

/// Создаёт supervisor и выполняет основной цикл до Stop или Shutdown.
///
/// В режиме Windows Service `status_handle` позволяет передавать SCM
/// состояние StopPending с растущими checkpoint. В dev-режиме SCM нет.
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
    // WindowsProcessLauncher формирует аргументы динамически. Этот spec нужен
    // для общего независимого от ОС интерфейса и mock-тестов.
    let spec = ProcessSpec {
        executable: session_host_path,
        args: Vec::new(),
    };

    let mut supervisor: Supervisor = SessionSupervisor::new(provider, launcher, spec);
    let service_instance_id = agent_core::config::new_service_instance_id().to_string();
    let network_cancellation = CancellationToken::new();
    let capture_broker = Arc::new(CaptureBroker::default());
    match agent_core::config::load_or_create_device_id(&agent_core::config::device_id_path()) {
        Ok(device_id) => {
            match crate::identity_store::load_or_create(
                &device_id.to_string(),
                &agent_core::config::device_certificate_path(),
                &agent_core::config::protected_device_key_path(),
            ) {
                Ok(identity) => {
                    let fingerprint = identity
                        .certificate_fingerprint()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>();
                    tracing::info!(
                        %device_id,
                        %service_instance_id,
                        certificate_fingerprint = fingerprint,
                        event = "SERVICE_IDENTITY_READY"
                    );
                    start_network(
                        device_id.to_string(),
                        Arc::new(identity),
                        network_cancellation.child_token(),
                        Arc::clone(&capture_broker),
                    );
                }
                Err(err) => {
                    tracing::error!(error = %err, %device_id, event = "SERVICE_IDENTITY_FAILED")
                }
            }
        }
        Err(err) => tracing::error!(error = %err, event = "SERVICE_IDENTITY_FAILED"),
    }

    let mut reconcile_tick = interval(RECONCILE_INTERVAL);
    reconcile_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let (conn_tx, mut conn_rx) = mpsc::unbounded_channel::<ConnectionEvent>();
    let mut connections = HashMap::new();
    let mut locked_sessions = HashSet::new();

    loop {
        tokio::select! {
            _ = reconcile_tick.tick() => {
                do_reconcile(&mut supervisor, &service_instance_id, &conn_tx, &mut connections, &capture_broker);
            }
            maybe_event = service_events.recv() => {
                match maybe_event {
                    Some(ServiceEvent::Stop) | Some(ServiceEvent::Shutdown) => {
                        network_cancellation.cancel();
                        graceful_shutdown(&mut supervisor, &connections, status_handle).await;
                        return;
                    }
                    Some(ServiceEvent::SessionLock(session_id)) => {
                        locked_sessions.insert(session_id);
                        tracing::info!(session_id, is_locked = true, event = "SESSION_LOCK_STATE_CHANGED");
                        do_reconcile(&mut supervisor, &service_instance_id, &conn_tx, &mut connections, &capture_broker);
                    }
                    Some(ServiceEvent::SessionUnlock(session_id)) => {
                        locked_sessions.remove(&session_id);
                        tracing::info!(session_id, is_locked = false, event = "SESSION_LOCK_STATE_CHANGED");
                        do_reconcile(&mut supervisor, &service_instance_id, &conn_tx, &mut connections, &capture_broker);
                    }
                    Some(_other) => {
                        do_reconcile(&mut supervisor, &service_instance_id, &conn_tx, &mut connections, &capture_broker);
                    }
                    None => {
                        // Без SCM продолжаем работу по reconcile timer.
                    }
                }
            }
            maybe_conn_event = conn_rx.recv() => {
                match maybe_conn_event {
                    Some(ConnectionEvent::Ready { session_id, pid }) => {
                        supervisor.notify_ipc_ready(session_id, pid, Instant::now());
                    }
                    Some(ConnectionEvent::Lost { pid }) => {
                        connections.remove(&pid);
                        if let Some(session_id) = capture_broker.clear(pid) {
                            tracing::warn!(session_id = %session_id, event = "REMOTE_CONTROL_REVOKED", reason = "session_host_disconnected");
                        }
                        for event in supervisor.notify_ipc_lost(pid, Instant::now()) {
                            log_supervisor_event(&event);
                        }
                    }
                    None => {}
                }
            }
        }
    }
}

fn start_network(
    device_id: String,
    identity: Arc<transport::DeviceIdentity>,
    cancellation: CancellationToken,
    capture_broker: Arc<CaptureBroker>,
) {
    tokio::spawn(async move {
        let bind_addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, transport::DEFAULT_CONTROL_PORT));
        let server = match transport::TlsControlServer::bind(bind_addr, &identity).await {
            Ok(server) => server,
            Err(err) => {
                tracing::error!(error = %err, event = "CONTROL_LISTENER_FAILED");
                return;
            }
        };
        let control_port = match server.local_addr() {
            Ok(addr) => addr.port(),
            Err(err) => {
                tracing::error!(error = %err, event = "CONTROL_LISTENER_FAILED");
                return;
            }
        };
        tracing::info!(port = control_port, event = "CONTROL_LISTENER_READY");

        let announcement = transport::new_announcement(
            device_id.clone(),
            std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_owned()),
            String::new(),
            env!("CARGO_PKG_VERSION").to_owned(),
            transport::local_ipv4()
                .map(|ip| ip.to_string())
                .unwrap_or_default(),
            control_port,
        );
        let discovery_cancel = cancellation.child_token();
        tokio::spawn(async move {
            if let Err(err) = transport::announce_loop(
                announcement,
                transport::DiscoveryConfig::default(),
                discovery_cancel,
            )
            .await
            {
                tracing::error!(error = %err, event = "DISCOVERY_FAILED");
            }
        });

        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break,
                accepted = server.accept() => match accepted {
                    Ok((connection, peer)) => {
                        let identity = Arc::clone(&identity);
                        let device_id = device_id.clone();
                        let capture_broker = Arc::clone(&capture_broker);
                        tokio::spawn(async move {
                            if let Err(err) = handle_control_connection(connection, peer, device_id, identity, capture_broker).await {
                                tracing::debug!(error = %err, event = "CONTROL_CONNECTION_CLOSED");
                            }
                        });
                    }
                    Err(err) => tracing::warn!(error = %err, event = "CONTROL_ACCEPT_FAILED"),
                }
            }
        }
    });
}

async fn handle_control_connection(
    mut connection: transport::ServerControlConnection,
    peer: SocketAddr,
    device_id: String,
    identity: Arc<transport::DeviceIdentity>,
    capture_broker: Arc<CaptureBroker>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let now = now_unix_ms();
    connection
        .send(&transport::build_device_hello(
            new_message_id(),
            now,
            device_id.clone(),
            std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_owned()),
            env!("CARGO_PKG_VERSION").to_owned(),
            "Windows".to_owned(),
        ))
        .await?;

    if let Some(material) = crate::identity_store::load_enrollment()? {
        let Some(hello) = connection.recv().await? else {
            return Ok(());
        };
        let verified = match transport::verify_teacher_hello(
            &hello,
            &material.issuer_public_key,
            &device_id,
            identity.certificate_der(),
            now_unix_ms(),
        ) {
            Ok(verified) => verified,
            Err(transport::HandshakeError::UpgradeRequired(required)) => {
                connection
                    .send(&protocol::network::Envelope {
                        protocol_version: protocol::network::PROTOCOL_VERSION,
                        message_id: new_message_id(),
                        timestamp_ms: now_unix_ms(),
                        payload: Some(protocol::network::envelope::Payload::UpgradeRequired(
                            required,
                        )),
                    })
                    .await?;
                return Ok(());
            }
            Err(err) => return Err(Box::new(err)),
        };
        tracing::info!(peer = %peer, teacher_session_id = %verified.teacher_session_id, event = "CONTROL_AUTHENTICATED");
        let status = protocol::network::Envelope {
            protocol_version: protocol::network::PROTOCOL_VERSION,
            message_id: new_message_id(),
            timestamp_ms: now_unix_ms(),
            payload: Some(protocol::network::envelope::Payload::DeviceStatus(
                protocol::network::DeviceStatus {
                    device_id: device_id.clone(),
                    state: protocol::network::DeviceOnlineState::Online as i32,
                    agent_version: env!("CARGO_PKG_VERSION").to_owned(),
                },
            )),
        };
        connection.send(&status).await?;
        serve_heartbeat(
            connection,
            device_id,
            verified.teacher_session_id,
            capture_broker,
        )
        .await?;
        return Ok(());
    }

    let Some(code) = crate::identity_store::load_pending_enrollment_code()? else {
        tracing::debug!(peer = %peer, event = "CONTROL_REJECTED_NOT_ENROLLED");
        return Ok(());
    };
    let request = protocol::network::Envelope {
        protocol_version: protocol::network::PROTOCOL_VERSION,
        message_id: new_message_id(),
        timestamp_ms: now,
        payload: Some(protocol::network::envelope::Payload::EnrollmentRequest(
            protocol::network::EnrollmentRequest {
                enrollment_code: code,
                device_id: device_id.clone(),
                device_public_key_der: identity.certificate_der().to_vec(),
                organization_id: "default".to_owned(),
                branch_id: "default".to_owned(),
                device_certificate_der: identity.certificate_der().to_vec(),
            },
        )),
    };
    connection.send(&request).await?;
    let Some(result) = connection.recv().await? else {
        return Ok(());
    };
    let Some(protocol::network::envelope::Payload::EnrollmentResult(result)) = result.payload
    else {
        return Ok(());
    };
    if !result.success || result.issuer_public_key_der.len() != 32 {
        tracing::warn!(peer = %peer, event = "ENROLLMENT_REJECTED");
        return Ok(());
    }
    let issuer_public_key: [u8; 32] = result.issuer_public_key_der.as_slice().try_into().unwrap();
    transport::DeviceCredential::decode_and_verify(
        &result.issued_credential,
        &issuer_public_key,
        &device_id,
        identity.certificate_der(),
        now_unix_ms(),
    )?;
    crate::identity_store::save_enrollment(&crate::identity_store::EnrollmentMaterial {
        credential: result.issued_credential,
        issuer_public_key,
    })?;
    tracing::info!(peer = %peer, event = "ENROLLMENT_ACCEPTED");
    Ok(())
}

async fn serve_heartbeat(
    connection: transport::ServerControlConnection,
    device_id: String,
    teacher_session_id: String,
    capture_broker: Arc<CaptureBroker>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let result = serve_heartbeat_loop(
        connection,
        device_id,
        teacher_session_id.clone(),
        Arc::clone(&capture_broker),
    )
    .await;

    if let Some(session_id) = capture_broker.release_remote(&teacher_session_id) {
        capture_broker.stop_remote("teacher_disconnected");
        tracing::info!(teacher_session_id = %teacher_session_id, session_id = %session_id, action = "REMOTE_CONTROL_STOPPED", result = "SUCCESS", reason = "connection_closed", event = "AUDIT");
    }

    result
}

async fn serve_heartbeat_loop(
    mut connection: transport::ServerControlConnection,
    device_id: String,
    teacher_session_id: String,
    capture_broker: Arc<CaptureBroker>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut tick = tokio::time::interval(agent_core::network::NETWORK_HEARTBEAT_INTERVAL);
    let mut stream_tick = tokio::time::interval(Duration::from_millis(50));
    stream_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut last_seen = std::time::Instant::now();
    let mut subscription: Option<agent_core::stream::ActiveSubscription> = None;
    let mut frame_clock: Option<agent_core::stream::FrameClock> = None;
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if last_seen.elapsed() > agent_core::network::NETWORK_OFFLINE_TIMEOUT {
                    if capture_broker.release_remote(&teacher_session_id).is_some() { capture_broker.stop_remote("teacher_disconnected"); }
                    return Ok(());
                }
                if subscription.is_some_and(|value| value.is_expired(now_unix_ms())) {
                    subscription = None;
                    frame_clock = None;
                    tracing::info!(event = "STREAM_SUBSCRIPTION_EXPIRED");
                }
                connection.send(&protocol::network::Envelope {
                    protocol_version: protocol::network::PROTOCOL_VERSION,
                    message_id: new_message_id(),
                    timestamp_ms: now_unix_ms(),
                    payload: Some(protocol::network::envelope::Payload::Heartbeat(protocol::network::Heartbeat {
                        sequence: last_seen.elapsed().as_secs(), sent_at_unix_ms: now_unix_ms(),
                    })),
                }).await?;
            }
            _ = stream_tick.tick() => {
                let Some(clock) = &mut frame_clock else { continue; };
                let now = now_unix_ms();
                let Some(sequence) = clock.take_due(now) else { continue; };
                let schedule = clock.schedule();
                let mode = match schedule.mode {
                    agent_core::stream::StreamVisibility::Visible => protocol::network::StreamMode::Thumbnail,
                    agent_core::stream::StreamVisibility::Selected => protocol::network::StreamMode::Selected,
                    agent_core::stream::StreamVisibility::Hidden => continue,
                };
                let quality = match mode {
                    protocol::network::StreamMode::Thumbnail => 60,
                    protocol::network::StreamMode::Selected => 80,
                    protocol::network::StreamMode::Unspecified => continue,
                };
                let payload = match capture_broker.capture(protocol::CaptureRequest {
                    display_id: 0,
                    max_width: schedule.max_width,
                    jpeg_quality: quality,
                }).await {
                    Ok(frame) => protocol::network::envelope::Payload::ScreenFrame(protocol::network::ScreenFrame {
                        device_id: device_id.clone(),
                        display_id: frame.display_id,
                        width: frame.width,
                        height: frame.height,
                        encoded_data: frame.encoded_data,
                        format: frame.format,
                        captured_at_unix_ms: now_unix_ms(),
                        mode: mode as i32,
                        sequence,
                    }),
                    Err(error) => protocol::network::envelope::Payload::CaptureError(protocol::network::CaptureError {
                        code: error.code,
                        message: error.message,
                    }),
                };
                connection.send(&protocol::network::Envelope {
                    protocol_version: protocol::network::PROTOCOL_VERSION,
                    message_id: new_message_id(),
                    timestamp_ms: now_unix_ms(),
                    payload: Some(payload),
                }).await?;
            }
            received = connection.recv() => {
                match received? {
                    Some(message) => match message.payload {
                        Some(protocol::network::envelope::Payload::Heartbeat(_)) => {
                            last_seen = std::time::Instant::now();
                            if let Some(value) = &mut subscription { value.refresh(now_unix_ms()); }
                        }
                        Some(protocol::network::envelope::Payload::StreamSubscribe(request)) => {
                            if request.device_id != device_id {
                                tracing::warn!(requested_device_id = request.device_id, event = "STREAM_SUBSCRIPTION_REJECTED");
                                continue;
                            }
                            let visibility = match protocol::network::StreamMode::try_from(request.mode) {
                                Ok(protocol::network::StreamMode::Thumbnail) => agent_core::stream::StreamVisibility::Visible,
                                Ok(protocol::network::StreamMode::Selected) => agent_core::stream::StreamVisibility::Selected,
                                _ => agent_core::stream::StreamVisibility::Hidden,
                            };
                            let schedule = agent_core::stream::negotiate_schedule(visibility, request.target_fps, request.max_width);
                            subscription = Some(agent_core::stream::ActiveSubscription::new(schedule, now_unix_ms()));
                            frame_clock = Some(agent_core::stream::FrameClock::new(schedule, now_unix_ms()));
                            tracing::info!(fps = schedule.fps, max_width = schedule.max_width, event = "STREAM_SUBSCRIBED");
                        }
                        Some(protocol::network::envelope::Payload::StreamUnsubscribe(_)) => {
                            subscription = None;
                            frame_clock = None;
                            tracing::info!(event = "STREAM_UNSUBSCRIBED");
                        }
                        Some(protocol::network::envelope::Payload::ScreenshotRequest(request)) => {
                            let payload = if request.device_id != device_id {
                                protocol::network::envelope::Payload::CaptureError(protocol::network::CaptureError {
                                    code: "DEVICE_MISMATCH".to_owned(), message: "запрос предназначен другому устройству".to_owned(),
                                })
                            } else {
                                match capture_broker.capture(protocol::CaptureRequest {
                                    display_id: request.display_id,
                                    max_width: 0,
                                    jpeg_quality: 80,
                                }).await {
                                    Ok(frame) => protocol::network::envelope::Payload::ScreenFrame(protocol::network::ScreenFrame {
                                        device_id: device_id.clone(), display_id: frame.display_id, width: frame.width, height: frame.height,
                                        encoded_data: frame.encoded_data, format: frame.format, captured_at_unix_ms: now_unix_ms(),
                                        mode: protocol::network::StreamMode::Selected as i32, sequence: 0,
                                    }),
                                    Err(error) => protocol::network::envelope::Payload::CaptureError(protocol::network::CaptureError { code: error.code, message: error.message }),
                                }
                            };
                            connection.send(&protocol::network::Envelope {
                                protocol_version: protocol::network::PROTOCOL_VERSION,
                                message_id: new_message_id(),
                                timestamp_ms: now_unix_ms(),
                                payload: Some(payload),
                            }).await?;
                        }
                        Some(protocol::network::envelope::Payload::Command(command)) => {
                            let command_id = command.command_id.clone();
                            let result = match capture_broker.admit_command(
                                &command_id,
                                command.expires_at_unix_ms,
                            ) {
                                CommandAdmission::Execute => {
                                    let result = execute_classroom_command(command, &capture_broker).await;
                                    capture_broker.complete_command(&command_id, result.clone());
                                    result
                                }
                                CommandAdmission::Replay(result) => result,
                                CommandAdmission::InProgress => command_result(
                                    command_id,
                                    false,
                                    "COMMAND_IN_PROGRESS",
                                    "команда с этим идентификатором ещё выполняется",
                                ),
                                CommandAdmission::Expired => command_result(
                                    command_id,
                                    false,
                                    "COMMAND_EXPIRED",
                                    "срок действия команды истёк или идентификатор пуст",
                                ),
                            };
                            tracing::info!(teacher_session_id = %teacher_session_id, command_id = %result.command_id, success = result.success, error_code = %result.error_code, action = "CLASSROOM_COMMAND", event = "AUDIT");
                            connection.send(&protocol::network::Envelope {
                                protocol_version: protocol::network::PROTOCOL_VERSION,
                                message_id: new_message_id(),
                                timestamp_ms: now_unix_ms(),
                                payload: Some(protocol::network::envelope::Payload::CommandResult(result)),
                            }).await?;
                        }
                        Some(protocol::network::envelope::Payload::RemoteControlStart(request)) => {
                            let session_id = new_message_id();
                            let payload = if request.device_id != device_id {
                                tracing::info!(teacher_session_id = %teacher_session_id, requested_device_id = %request.device_id, action = "REMOTE_CONTROL_STARTED", result = "DENIED", reason = "device_mismatch", event = "AUDIT");
                                protocol::network::envelope::Payload::RemoteControlStopped(protocol::network::RemoteControlStopped { device_id: device_id.clone(), reason: "denied".to_owned() })
                            } else if let Err(error) = capture_broker.claim_remote(teacher_session_id.clone(), session_id.clone()) {
                                tracing::info!(teacher_session_id = %teacher_session_id, action = "REMOTE_CONTROL_STARTED", result = "DENIED", reason = ?error, event = "AUDIT");
                                protocol::network::envelope::Payload::RemoteControlStopped(protocol::network::RemoteControlStopped { device_id: device_id.clone(), reason: "denied".to_owned() })
                            } else if let Err(error) = capture_broker.start_remote(session_id.clone()).await {
                                let _ = capture_broker.release_remote(&teacher_session_id);
                                tracing::warn!(teacher_session_id = %teacher_session_id, session_id = %session_id, code = error.code, action = "REMOTE_CONTROL_STARTED", result = "ERROR", event = "AUDIT");
                                protocol::network::envelope::Payload::RemoteControlStopped(protocol::network::RemoteControlStopped { device_id: device_id.clone(), reason: "error".to_owned() })
                            } else {
                                tracing::info!(teacher_session_id = %teacher_session_id, session_id = %session_id, action = "REMOTE_CONTROL_STARTED", result = "SUCCESS", event = "AUDIT");
                                protocol::network::envelope::Payload::RemoteControlStarted(protocol::network::RemoteControlStarted { device_id: device_id.clone(), session_id })
                            };
                            connection.send(&protocol::network::Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: new_message_id(), timestamp_ms: now_unix_ms(), payload: Some(payload) }).await?;
                        }
                        Some(protocol::network::envelope::Payload::RemoteControlStop(request)) => {
                            let payload = if request.device_id != device_id {
                                tracing::info!(teacher_session_id = %teacher_session_id, requested_device_id = %request.device_id, action = "REMOTE_CONTROL_STOPPED", result = "DENIED", reason = "device_mismatch", event = "AUDIT");
                                protocol::network::envelope::Payload::RemoteControlStopped(protocol::network::RemoteControlStopped { device_id: device_id.clone(), reason: "denied".to_owned() })
                            } else {
                                match capture_broker.release_remote(&teacher_session_id) {
                                    Some(session_id) => {
                                    capture_broker.stop_remote("teacher_stopped");
                                    tracing::info!(teacher_session_id = %teacher_session_id, session_id = %session_id, action = "REMOTE_CONTROL_STOPPED", result = "SUCCESS", event = "AUDIT");
                                    protocol::network::envelope::Payload::RemoteControlStopped(protocol::network::RemoteControlStopped { device_id: device_id.clone(), reason: "teacher_stopped".to_owned() })
                                    }
                                    None => {
                                        tracing::info!(teacher_session_id = %teacher_session_id, action = "REMOTE_CONTROL_STOPPED", result = "DENIED", reason = "not_owner", event = "AUDIT");
                                        protocol::network::envelope::Payload::RemoteControlStopped(protocol::network::RemoteControlStopped { device_id: device_id.clone(), reason: "denied".to_owned() })
                                    }
                                }
                            };
                            connection.send(&protocol::network::Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: new_message_id(), timestamp_ms: now_unix_ms(), payload: Some(payload) }).await?;
                        }
                        Some(protocol::network::envelope::Payload::RemoteInputEvent(event)) => {
                            if event.device_id != device_id || !capture_broker.accepts_remote_input(&teacher_session_id) {
                                tracing::warn!(event = "REMOTE_INPUT_REJECTED", reason = "not_remote_owner");
                            } else if let Some(event) = to_local_remote_input(event) {
                                if let Err(error) = capture_broker.send_remote_input(event) {
                                    tracing::warn!(code = error.code, event = "REMOTE_INPUT_REJECTED");
                                }
                            } else {
                                tracing::warn!(event = "REMOTE_INPUT_REJECTED", reason = "invalid_event");
                            }
                        }
                        _ => {}
                    },
                    None => {
                        if capture_broker.release_remote(&teacher_session_id).is_some() { capture_broker.stop_remote("teacher_disconnected"); }
                        return Ok(());
                    },
                }
            }
        }
    }
}

/// Интервал обновления checkpoint в состоянии StopPending. Он должен быть
/// заметно короче `wait_hint`, иначе SCM сочтёт службу зависшей.
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
    connections: &HashMap<u32, mpsc::UnboundedSender<ConnectionCommand>>,
    status_handle: Option<ServiceStatusHandle>,
) {
    tracing::info!(event = "SERVICE_STOPPING");

    // Сразу сообщаем StopPending, чтобы SCM видел переход состояния и не
    // считал службу зависшей.
    let mut checkpoint = 1;
    report_stop_pending(status_handle, checkpoint);

    // Сначала просим Session Host завершиться через протокол. После grace
    // period принудительно завершаем только PID, которым владеет supervisor.
    let mut managed_pid = None;
    if let agent_core::supervisor::SupervisorState::Running { pid, .. }
    | agent_core::supervisor::SupervisorState::WaitingForIpc { pid, .. } = supervisor.state()
    {
        managed_pid = Some(pid);
        if let Some(commands) = connections.get(&pid) {
            let _ = commands.send(ConnectionCommand::Shutdown);
        }
    }

    let mut remaining = SHUTDOWN_GRACE_PERIOD;
    while !remaining.is_zero() {
        let step = remaining.min(STOP_PENDING_CHECKPOINT_INTERVAL);
        tokio::time::sleep(step).await;
        remaining -= step;
        checkpoint += 1;
        report_stop_pending(status_handle, checkpoint);

        if managed_pid.is_some_and(|pid| !supervisor.launcher().is_alive(pid)) {
            managed_pid = None;
            break;
        }
    }

    if let Some(pid) = managed_pid {
        tracing::warn!(pid, event = "SESSION_HOST_FORCE_TERMINATING");
        let _ = supervisor.launcher().terminate(pid);
    }
}
