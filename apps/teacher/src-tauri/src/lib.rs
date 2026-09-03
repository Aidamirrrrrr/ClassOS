//! Минимальный backend Teacher Console для сетевого milestone T1.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agent_core::network::{DEFAULT_ENROLLMENT_TTL, EnrollmentAuthority, EnrollmentContext};
use protocol::network::{envelope, EnrollmentErrorCode, EnrollmentResult, Envelope};
use serde::Serialize;
use tauri::State;
use transport::{discovery, DeviceTransport, TeacherAuthority, TlsClient};

struct AppState {
    authority: Mutex<TeacherAuthority>,
    enrollment: Mutex<EnrollmentAuthority>,
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
    let result = Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("enrollment-result-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(envelope::Payload::EnrollmentResult(EnrollmentResult { success: true, error_code: EnrollmentErrorCode::Unspecified as i32, issued_credential, issuer_public_key_der: issuer_public_key, expires_at_unix_ms: expires })) };
    connection.send(&result).await.map_err(|error| error.to_string())?;
    Ok("устройство успешно зарегистрировано".to_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState { authority: Mutex::new(load_authority()), enrollment: Mutex::new(EnrollmentAuthority::default()) })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![create_enrollment_code, discover_device, enroll_device])
        .run(tauri::generate_context!())
        .expect("не удалось запустить Teacher Console");
}
