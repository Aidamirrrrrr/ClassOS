//! Минимальный backend Teacher Console для сетевого milestone T1.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agent_core::network::{DEFAULT_ENROLLMENT_TTL, EnrollmentAuthority, EnrollmentContext};
use protocol::network::{envelope, EnrollmentErrorCode, EnrollmentResult, Envelope};
use serde::Serialize;
use tauri::State;
use transport::{discovery, build_teacher_hello, DeviceCredential, DeviceTransport, TeacherAuthority, TlsClient};
use sha2::{Digest, Sha256};

struct AppState {
    authority: Mutex<TeacherAuthority>,
    enrollment: Mutex<EnrollmentAuthority>,
    devices: Mutex<HashMap<String, EnrolledDevice>>,
}

struct EnrolledDevice {
    credential: DeviceCredential,
    fingerprint: [u8; 32],
    ip: String,
    control_port: u16,
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

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64
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
async fn request_screenshot(state: State<'_, AppState>, device_id: String, display_id: u32) -> Result<Vec<u8>, String> {
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
    connection.send(&protocol::network::Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("screenshot-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(protocol::network::envelope::Payload::ScreenshotRequest(protocol::network::ScreenshotRequest { device_id, display_id })) }).await.map_err(|error| error.to_string())?;
    match connection.recv().await.map_err(|error| error.to_string())?.and_then(|message| message.payload) {
        Some(protocol::network::envelope::Payload::ScreenFrame(frame)) => Ok(frame.encoded_data),
        Some(protocol::network::envelope::Payload::CaptureError(error)) => Err(format!("{}: {}", error.code, error.message)),
        _ => Err("устройство вернуло неожиданный ответ на ScreenshotRequest".to_owned()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState { authority: Mutex::new(load_authority()), enrollment: Mutex::new(EnrollmentAuthority::default()), devices: Mutex::new(HashMap::new()) })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![create_enrollment_code, discover_device, enroll_device, request_screenshot])
        .run(tauri::generate_context!())
        .expect("не удалось запустить Teacher Console");
}
