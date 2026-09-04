//! Минимальный backend Teacher Console для сетевого milestone T1.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agent_core::network::{DEFAULT_ENROLLMENT_TTL, EnrollmentAuthority, EnrollmentContext};
use protocol::network::{envelope, EnrollmentErrorCode, EnrollmentResult, Envelope};
use serde::Serialize;
use tauri::http::{Response, StatusCode, header};
use tauri::{Emitter, State};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use transport::{
    DeviceCredential, DeviceTransport, TeacherAuthority, TlsClient, build_teacher_hello, discovery,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

struct AppState {
    authority: Mutex<TeacherAuthority>,
    enrollment: Mutex<EnrollmentAuthority>,
    devices: Mutex<HashMap<String, EnrolledDevice>>,
    frames: Arc<Mutex<HashMap<String, StoredFrame>>>,
    streams: Arc<Mutex<HashMap<String, CancellationToken>>>,
    remote_controls: Arc<Mutex<HashMap<String, RemoteControlHandle>>>,
}

struct RemoteControlHandle { sender: mpsc::UnboundedSender<Envelope>, cancellation: CancellationToken }

#[derive(Clone)]
struct EnrolledDevice {
    credential: DeviceCredential,
    fingerprint: [u8; 32],
    ip: String,
    control_port: u16,
}

struct StoredFrame {
    jpeg: Vec<u8>,
}

#[derive(Debug, Serialize)]
struct DeviceView {
    device_id: String,
    hostname: String,
    ip: String,
    control_port: u32,
    room_hint: String,
    agent_version: String,
}

#[derive(Debug, Serialize)]
struct EnrollmentCodeView {
    code: String,
    expires_at_unix_ms: i64,
}

#[derive(Debug, Serialize)]
struct CommandResultView {
    device_id: String,
    command_id: String,
    success: bool,
    error_code: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct FrameReady {
    device_id: String,
    sequence: u32,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
}

fn frame_url(device_id: &str) -> String {
    #[cfg(windows)]
    {
        format!("http://classos-frame.localhost/frames/{device_id}")
    }
    #[cfg(not(windows))]
    {
        format!("classos-frame://localhost/frames/{device_id}")
    }
}

fn store_frame(
    frames: &Arc<Mutex<HashMap<String, StoredFrame>>>,
    device_id: String,
    jpeg: Vec<u8>,
) {
    if let Ok(mut frames) = frames.lock() {
        frames.insert(device_id, StoredFrame { jpeg });
    }
}

fn authority_path() -> PathBuf {
    std::env::var_os("CLASSOS_TEACHER_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("teacher-authority.key")
}

fn load_authority() -> TeacherAuthority {
    let path = authority_path();
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(secret) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return TeacherAuthority::from_secret(&secret);
        }
    }
    let authority = TeacherAuthority::generate().expect("криптографическая случайность доступна");
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let _ = std::fs::write(path, authority.secret_bytes());
    authority
}

#[tauri::command]
fn create_enrollment_code(state: State<'_, AppState>) -> Result<EnrollmentCodeView, String> {
    let mut authority = state.enrollment.lock().map_err(|_| "состояние занято")?;
    let code = authority.issue(EnrollmentContext::default(), now_ms(), DEFAULT_ENROLLMENT_TTL);
    Ok(EnrollmentCodeView { code: code.value, expires_at_unix_ms: code.expires_at_unix_ms })
}

#[tauri::command]
async fn discover_device() -> Result<DeviceView, String> {
    let received = tokio::time::timeout(Duration::from_secs(8), discovery::listen_once(Default::default()))
        .await.map_err(|_| "время ожидания discovery истекло".to_owned())?
        .map_err(|error| error.to_string())?;
    let source_ip = received.source.ip().to_string();
    let announcement = received.announcement;
    Ok(DeviceView { device_id: announcement.device_id, hostname: announcement.hostname, ip: source_ip, control_port: announcement.control_port, room_hint: announcement.room_hint, agent_version: announcement.agent_version })
}

#[tauri::command]
async fn enroll_device(state: State<'_, AppState>, device_id: String, ip: String, control_port: u32, code: String) -> Result<String, String> {
    let port = u16::try_from(control_port).map_err(|_| "некорректный control_port")?;
    let addr = SocketAddr::new(ip.parse::<IpAddr>().map_err(|_| "некорректный IP-адрес")?, port);
    let client = TlsClient::bootstrap(&device_id).map_err(|error| error.to_string())?;
    let mut connection = client.connect(addr).await.map_err(|error| error.to_string())?;
    let fingerprint: [u8; 32] = Sha256::digest(connection.peer_certificate_der().map_err(|error| error.to_string())?).into();
    let hello = connection.recv().await.map_err(|error| error.to_string())?.ok_or_else(|| "устройство закрыло соединение до DeviceHello".to_owned())?;
    let device_hello = match hello.payload { Some(envelope::Payload::DeviceHello(value)) => value, _ => return Err("первым сообщением ожидался DeviceHello".to_owned()) };
    let request = connection.recv().await.map_err(|error| error.to_string())?.ok_or_else(|| "устройство закрыло соединение до EnrollmentRequest".to_owned())?;
    let request = match request.payload { Some(envelope::Payload::EnrollmentRequest(value)) => value, _ => return Err("ожидался EnrollmentRequest".to_owned()) };
    if request.device_id != device_id || device_hello.device_id != device_id { return Err("идентификатор устройства не совпадает с discovery".to_owned()); }
    { let mut enrollment = state.enrollment.lock().map_err(|_| "состояние занято")?; enrollment.consume(&code, &EnrollmentContext::default(), now_ms()).map_err(|error| error.to_string())?; }
    let expires = now_ms().saturating_add(Duration::from_secs(30 * 24 * 3600).as_millis() as i64);
    let (issued_credential, issuer_public_key) = {
        let authority = state.authority.lock().map_err(|_| "состояние занято")?;
        let credential = authority.issue_device_credential(&device_id, &request.device_certificate_der, expires);
        (credential.encode(), authority.public_key().to_vec())
    };
    let issuer_public_key_array: [u8; 32] = issuer_public_key.as_slice().try_into().map_err(|_| "некорректный ключ issuer")?;
    let credential = DeviceCredential::decode_and_verify(&issued_credential, &issuer_public_key_array, &device_id, &connection.peer_certificate_der().map_err(|error| error.to_string())?, now_ms()).map_err(|error| error.to_string())?;
    let result = Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("enrollment-result-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(envelope::Payload::EnrollmentResult(EnrollmentResult { success: true, error_code: EnrollmentErrorCode::Unspecified as i32, issued_credential, issuer_public_key_der: issuer_public_key, expires_at_unix_ms: expires })) };
    connection.send(&result).await.map_err(|error| error.to_string())?;
    state.devices.lock().map_err(|_| "состояние занято")?.insert(device_id, EnrolledDevice { credential, fingerprint, ip, control_port: port });
    Ok("устройство успешно зарегистрировано".to_owned())
}

#[tauri::command]
async fn request_screenshot(
    state: State<'_, AppState>,
    device_id: String,
    display_id: u32,
) -> Result<String, String> {
    let (credential, fingerprint, ip, port) = {
        let devices = state.devices.lock().map_err(|_| "состояние занято")?;
        let device = devices.get(&device_id).ok_or_else(|| "устройство ещё не зарегистрировано в этой сессии".to_owned())?;
        (device.credential.clone(), device.fingerprint, device.ip.clone(), device.control_port)
    };
    let client = TlsClient::pinned(&device_id, fingerprint).map_err(|error| error.to_string())?;
    let mut connection = client.connect(SocketAddr::new(ip.parse::<IpAddr>().map_err(|_| "некорректный IP-адрес")?, port)).await.map_err(|error| error.to_string())?;
    let _device_hello = connection.recv().await.map_err(|error| error.to_string())?.ok_or_else(|| "устройство закрыло соединение".to_owned())?;
    let teacher_hello = {
        let authority = state.authority.lock().map_err(|_| "состояние занято")?;
        build_teacher_hello(&authority, &credential, format!("teacher-{}", now_ms()), format!("hello-{}", now_ms()), now_ms())
    };
    connection.send(&teacher_hello).await.map_err(|error| error.to_string())?;
    let _status = connection.recv().await.map_err(|error| error.to_string())?.ok_or_else(|| "устройство не подтвердило авторизацию".to_owned())?;
    connection.send(&protocol::network::Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("screenshot-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(protocol::network::envelope::Payload::ScreenshotRequest(protocol::network::ScreenshotRequest { device_id: device_id.clone(), display_id })) }).await.map_err(|error| error.to_string())?;
    match connection.recv().await.map_err(|error| error.to_string())?.and_then(|message| message.payload) {
        Some(protocol::network::envelope::Payload::ScreenFrame(frame)) => {
            store_frame(&state.frames, device_id.clone(), frame.encoded_data);
            Ok(frame_url(&device_id))
        }
        Some(protocol::network::envelope::Payload::CaptureError(error)) => Err(format!("{}: {}", error.code, error.message)),
        _ => Err("устройство вернуло неожиданный ответ на ScreenshotRequest".to_owned()),
    }
}

#[tauri::command]
async fn start_stream(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    device_id: String,
    selected: bool,
) -> Result<String, String> {
    let (credential, fingerprint, ip, control_port, authority_secret) = {
        let devices = state.devices.lock().map_err(|_| "состояние занято")?;
        let device = devices
            .get(&device_id)
            .ok_or_else(|| "устройство ещё не зарегистрировано в этой сессии".to_owned())?;
        let authority = state.authority.lock().map_err(|_| "состояние занято")?;
        (
            device.credential.clone(),
            device.fingerprint,
            device.ip.clone(),
            device.control_port,
            authority.secret_bytes(),
        )
    };
    let cancellation = CancellationToken::new();
    if let Ok(mut streams) = state.streams.lock() {
        if let Some(previous) = streams.insert(device_id.clone(), cancellation.clone()) {
            previous.cancel();
        }
    }
    let frames = Arc::clone(&state.frames);
    let task_device_id = device_id.clone();
    tokio::spawn(async move {
        let result: Result<(), String> = async {
            let address = SocketAddr::new(
                ip.parse::<IpAddr>()
                    .map_err(|_| "некорректный IP-адрес".to_owned())?,
                control_port,
            );
            let client = TlsClient::pinned(&task_device_id, fingerprint)
                .map_err(|error| error.to_string())?;
            let mut connection = client.connect(address).await.map_err(|error| error.to_string())?;
            connection
                .recv()
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "устройство закрыло соединение".to_owned())?;
            let authority = TeacherAuthority::from_secret(&authority_secret);
            let teacher_hello = build_teacher_hello(
                &authority,
                &credential,
                format!("teacher-{}", now_ms()),
                format!("hello-{}", now_ms()),
                now_ms(),
            );
            connection
                .send(&teacher_hello)
                .await
                .map_err(|error| error.to_string())?;
            connection
                .recv()
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "устройство не подтвердило авторизацию".to_owned())?;
            let mode = if selected {
                protocol::network::StreamMode::Selected
            } else {
                protocol::network::StreamMode::Thumbnail
            };
            connection
                .send(&protocol::network::Envelope {
                    protocol_version: protocol::network::PROTOCOL_VERSION,
                    message_id: format!("stream-subscribe-{}", now_ms()),
                    timestamp_ms: now_ms(),
                    payload: Some(protocol::network::envelope::Payload::StreamSubscribe(
                        protocol::network::StreamSubscribe {
                            device_id: task_device_id.clone(),
                            mode: mode as i32,
                            target_fps: if selected { 12 } else { 1 },
                            max_width: if selected { 1_920 } else { 640 },
                        },
                    )),
                })
                .await
                .map_err(|error| error.to_string())?;
            let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => {
                        let _ = connection.send(&protocol::network::Envelope {
                            protocol_version: protocol::network::PROTOCOL_VERSION,
                            message_id: format!("stream-unsubscribe-{}", now_ms()),
                            timestamp_ms: now_ms(),
                            payload: Some(protocol::network::envelope::Payload::StreamUnsubscribe(
                                protocol::network::StreamUnsubscribe { device_id: task_device_id.clone() },
                            )),
                        }).await;
                        return Ok(());
                    }
                    _ = heartbeat.tick() => {
                        connection.send(&protocol::network::Envelope {
                            protocol_version: protocol::network::PROTOCOL_VERSION,
                            message_id: format!("heartbeat-{}", now_ms()),
                            timestamp_ms: now_ms(),
                            payload: Some(protocol::network::envelope::Payload::Heartbeat(protocol::network::Heartbeat {
                                sequence: now_ms() as u64,
                                sent_at_unix_ms: now_ms(),
                            })),
                        }).await.map_err(|error| error.to_string())?;
                    }
                    received = connection.recv() => {
                        let message = received.map_err(|error| error.to_string())?.ok_or_else(|| "устройство закрыло stream".to_owned())?;
                        if let Some(protocol::network::envelope::Payload::ScreenFrame(frame)) = message.payload {
                            let sequence = frame.sequence;
                            store_frame(&frames, task_device_id.clone(), frame.encoded_data);
                            let _ = app.emit("stream-frame-ready", FrameReady { device_id: task_device_id.clone(), sequence });
                        }
                    }
                }
            }
        }
        .await;
        if let Err(error) = result {
            let _ = app.emit("stream-status", error);
        }
    });
    Ok(frame_url(&device_id))
}

#[tauri::command]
fn stop_stream(state: State<'_, AppState>, device_id: String) {
    if let Ok(mut streams) = state.streams.lock() {
        if let Some(cancellation) = streams.remove(&device_id) {
            cancellation.cancel();
        }
    }
}

fn command_body(kind: &str, value: &str) -> Result<protocol::network::command::Body, String> {
    use protocol::network::command::Body;
    match kind {
        "lock" => Ok(Body::LockDevice(protocol::network::LockDevice {})),
        "unlock" => Ok(Body::UnlockDevice(protocol::network::UnlockDevice {})),
        "message" if !value.is_empty() => Ok(Body::ShowMessage(protocol::network::ShowMessage { text: value.to_owned() })),
        "application" => Ok(Body::LaunchApplication(protocol::network::LaunchApplication { application_id: value.to_owned() })),
        "url" => Ok(Body::OpenUrl(protocol::network::OpenUrl { url: value.to_owned() })),
        "restart" => Ok(Body::RestartDevice(protocol::network::RestartDevice {})),
        "shutdown" => Ok(Body::ShutdownDevice(protocol::network::ShutdownDevice {})),
        _ => Err("неизвестная или неполная classroom-команда".to_owned()),
    }
}

async fn dispatch_command_to_device(
    device_id: String,
    device: EnrolledDevice,
    authority_secret: [u8; 32],
    kind: String,
    value: String,
) -> CommandResultView {
    let command_id = Uuid::new_v4().to_string();
    let result: Result<protocol::network::CommandResult, String> = async {
        let body = command_body(&kind, &value)?;
        let client = TlsClient::pinned(&device_id, device.fingerprint).map_err(|error| error.to_string())?;
        let address = SocketAddr::new(device.ip.parse::<IpAddr>().map_err(|_| "некорректный IP-адрес".to_owned())?, device.control_port);
        let mut connection = client.connect(address).await.map_err(|error| error.to_string())?;
        connection.recv().await.map_err(|error| error.to_string())?.ok_or_else(|| "устройство закрыло соединение".to_owned())?;
        let authority = TeacherAuthority::from_secret(&authority_secret);
        connection.send(&build_teacher_hello(&authority, &device.credential, format!("teacher-{}", now_ms()), format!("hello-{}", now_ms()), now_ms())).await.map_err(|error| error.to_string())?;
        connection.recv().await.map_err(|error| error.to_string())?.ok_or_else(|| "устройство не подтвердило авторизацию".to_owned())?;
        connection.send(&Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("command-{}", command_id), timestamp_ms: now_ms(), payload: Some(envelope::Payload::Command(protocol::network::Command { command_id: command_id.clone(), expires_at_unix_ms: now_ms().saturating_add(30_000), body: Some(body) })) }).await.map_err(|error| error.to_string())?;
        match connection.recv().await.map_err(|error| error.to_string())?.and_then(|message| message.payload) {
            Some(envelope::Payload::CommandResult(result)) => Ok(result),
            _ => Err("устройство вернуло неожиданный ответ на classroom-команду".to_owned()),
        }
    }.await;
    match result {
        Ok(result) => CommandResultView { device_id, command_id: result.command_id, success: result.success, error_code: result.error_code, message: result.message },
        Err(message) => CommandResultView { device_id, command_id, success: false, error_code: "COMMAND_DELIVERY_FAILED".to_owned(), message },
    }
}

#[tauri::command]
async fn dispatch_classroom_command(
    state: State<'_, AppState>,
    device_ids: Vec<String>,
    kind: String,
    value: String,
) -> Result<Vec<CommandResultView>, String> {
    if device_ids.is_empty() { return Err("не выбраны устройства".to_owned()); }
    command_body(&kind, &value)?;
    let (devices, authority_secret) = {
        let stored = state.devices.lock().map_err(|_| "состояние занято")?;
        let devices = device_ids.into_iter().filter_map(|id| stored.get(&id).cloned().map(|device| (id, device))).collect::<Vec<_>>();
        let authority = state.authority.lock().map_err(|_| "состояние занято")?;
        (devices, authority.secret_bytes())
    };
    let mut tasks = JoinSet::new();
    for (device_id, device) in devices {
        tasks.spawn(dispatch_command_to_device(device_id, device, authority_secret, kind.clone(), value.clone()));
    }
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        results.push(result.map_err(|error| error.to_string())?);
    }
    Ok(results)
}

#[tauri::command]
async fn start_remote_control(state: State<'_, AppState>, device_id: String) -> Result<(), String> {
    let (credential, fingerprint, ip, port, secret) = {
        let devices = state.devices.lock().map_err(|_| "состояние занято")?;
        let device = devices.get(&device_id).ok_or_else(|| "устройство ещё не зарегистрировано".to_owned())?;
        let authority = state.authority.lock().map_err(|_| "состояние занято")?;
        (device.credential.clone(), device.fingerprint, device.ip.clone(), device.control_port, authority.secret_bytes())
    };
    let cancellation = CancellationToken::new();
    let (sender, mut receiver) = mpsc::unbounded_channel::<Envelope>();
    let (ready_tx, ready_rx) = oneshot::channel();
    let task_device_id = device_id.clone();
    let task_cancellation = cancellation.clone();
    tokio::spawn(async move {
        let mut ready_tx = Some(ready_tx);
        let result: Result<(), String> = async {
            let address = SocketAddr::new(ip.parse::<IpAddr>().map_err(|_| "некорректный IP-адрес".to_owned())?, port);
            let client = TlsClient::pinned(&task_device_id, fingerprint).map_err(|error| error.to_string())?;
            let mut connection = client.connect(address).await.map_err(|error| error.to_string())?;
            connection.recv().await.map_err(|error| error.to_string())?.ok_or_else(|| "устройство закрыло соединение".to_owned())?;
            let authority = TeacherAuthority::from_secret(&secret);
            connection.send(&build_teacher_hello(&authority, &credential, format!("teacher-{}", now_ms()), format!("hello-{}", now_ms()), now_ms())).await.map_err(|error| error.to_string())?;
            connection.recv().await.map_err(|error| error.to_string())?.ok_or_else(|| "устройство не подтвердило авторизацию".to_owned())?;
            connection.send(&Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("remote-start-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(envelope::Payload::RemoteControlStart(protocol::network::RemoteControlStart { device_id: task_device_id.clone() })) }).await.map_err(|error| error.to_string())?;
            match connection.recv().await.map_err(|error| error.to_string())?.and_then(|message| message.payload) {
                Some(envelope::Payload::RemoteControlStarted(_)) => { if let Some(sender) = ready_tx.take() { let _ = sender.send(Ok(())); } }
                Some(envelope::Payload::RemoteControlStopped(value)) => { if let Some(sender) = ready_tx.take() { let _ = sender.send(Err(format!("remote control отклонён: {}", value.reason))); } return Ok(()); }
                _ => { if let Some(sender) = ready_tx.take() { let _ = sender.send(Err("устройство вернуло неожиданный ответ на remote start".to_owned())); } return Ok(()); }
            }
            let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
            loop { tokio::select! {
                _ = task_cancellation.cancelled() => { let _ = connection.send(&Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("remote-stop-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(envelope::Payload::RemoteControlStop(protocol::network::RemoteControlStop { device_id: task_device_id.clone() })) }).await; return Ok(()); }
                _ = heartbeat.tick() => connection.send(&Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("heartbeat-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(envelope::Payload::Heartbeat(protocol::network::Heartbeat { sequence: now_ms() as u64, sent_at_unix_ms: now_ms() })) }).await.map_err(|error| error.to_string())?,
                message = receiver.recv() => match message { Some(message) => connection.send(&message).await.map_err(|error| error.to_string())?, None => return Ok(()) },
                received = connection.recv() => { if received.map_err(|error| error.to_string())?.is_none() { return Ok(()); } }
            }}
        }.await;
        if let Err(error) = result { if let Some(sender) = ready_tx.take() { let _ = sender.send(Err(error)); } }
    });
    tokio::time::timeout(Duration::from_secs(4), ready_rx).await.map_err(|_| "время ожидания remote control истекло".to_owned())?.map_err(|_| "remote control task завершилась".to_owned())??;
    state.remote_controls.lock().map_err(|_| "состояние занято")?.insert(device_id, RemoteControlHandle { sender, cancellation });
    Ok(())
}

#[tauri::command]
fn stop_remote_control(state: State<'_, AppState>, device_id: String) {
    if let Ok(mut controls) = state.remote_controls.lock() { if let Some(handle) = controls.remove(&device_id) { handle.cancellation.cancel(); } }
}

#[tauri::command]
fn send_remote_mouse_move(state: State<'_, AppState>, device_id: String, x: f32, y: f32) -> Result<(), String> {
    send_remote_event(&state, device_id, protocol::network::remote_input_event::Event::MouseMove(protocol::network::MouseMove { x, y }))
}

fn send_remote_event(state: &AppState, device_id: String, event: protocol::network::remote_input_event::Event) -> Result<(), String> {
    let handle = state.remote_controls.lock().map_err(|_| "состояние занято")?.get(&device_id).map(|value| value.sender.clone()).ok_or_else(|| "remote control не активен".to_owned())?;
    handle.send(Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("input-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(envelope::Payload::RemoteInputEvent(protocol::network::RemoteInputEvent { device_id, event: Some(event) })) }).map_err(|_| "control-соединение закрыто".to_owned())
}

#[tauri::command]
fn send_remote_mouse_button(state: State<'_, AppState>, device_id: String, button: i32, is_down: bool, x: f32, y: f32) -> Result<(), String> { send_remote_event(&state, device_id, protocol::network::remote_input_event::Event::MouseButton(protocol::network::MouseButton { button, is_down, x, y })) }
#[tauri::command]
fn send_remote_wheel(state: State<'_, AppState>, device_id: String, delta: i32) -> Result<(), String> { send_remote_event(&state, device_id, protocol::network::remote_input_event::Event::MouseWheel(protocol::network::MouseWheel { delta })) }
#[tauri::command]
fn send_remote_key(state: State<'_, AppState>, device_id: String, virtual_key_code: u32, is_down: bool) -> Result<(), String> { send_remote_event(&state, device_id, protocol::network::remote_input_event::Event::KeyEvent(protocol::network::KeyEvent { virtual_key_code, is_down })) }

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let frames = Arc::new(Mutex::new(HashMap::new()));
    let stream_frames = Arc::clone(&frames);
    tauri::Builder::default()
        .manage(AppState {
            authority: Mutex::new(load_authority()),
            enrollment: Mutex::new(EnrollmentAuthority::default()),
            devices: Mutex::new(HashMap::new()),
            frames,
            streams: Arc::new(Mutex::new(HashMap::new())),
            remote_controls: Arc::new(Mutex::new(HashMap::new())),
        })
        .register_uri_scheme_protocol("classos-frame", move |_, request| {
            let device_id = request.uri().path().strip_prefix("/frames/").unwrap_or_default();
            let jpeg = stream_frames
                .lock()
                .ok()
                .and_then(|frames| frames.get(device_id).map(|frame| frame.jpeg.clone()));
            match jpeg {
                Some(jpeg) => Response::builder()
                    .header(header::CONTENT_TYPE, "image/jpeg")
                    .header(header::CACHE_CONTROL, "no-store")
                    .body(jpeg)
                    .unwrap(),
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Vec::new())
                    .unwrap(),
            }
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            create_enrollment_code,
            discover_device,
            enroll_device,
            request_screenshot,
            start_stream,
            stop_stream,
            dispatch_classroom_command,
            start_remote_control,
            stop_remote_control,
            send_remote_mouse_move,
            send_remote_mouse_button,
            send_remote_wheel,
            send_remote_key
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить Teacher Console");
}
