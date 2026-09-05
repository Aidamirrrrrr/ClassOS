//! Backend Teacher Console: discovery, enrollment, экраны, команды и
//! интеграция с Cloud v0.

mod cloud;

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agent_core::network::{EnrollmentAuthority, EnrollmentContext, DEFAULT_ENROLLMENT_TTL};
use protocol::network::{envelope, EnrollmentErrorCode, EnrollmentResult, Envelope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::http::{header, Response, StatusCode};
use tauri::{Emitter, State};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use transport::{
    build_teacher_hello, discovery, DeviceCredential, DeviceTransport, SignedLease,
    TeacherAuthority, TlsClient,
};
use uuid::Uuid;

struct AppState {
    authority: Mutex<TeacherAuthority>,
    /// Classroom lease, выданный Cloud текущему преподавателю (ADR-0016).
    ///
    /// Пусто, пока Cloud не настроен: устройства из локального enrollment
    /// (ADR-0007) lease не требуют, а cloud-enrolled устройство без него
    /// откажет в подключении — и это правильное поведение, а не ошибка UI.
    lease: Mutex<Option<SignedLease>>,
    /// Отмена фоновой задачи обнаружения, если она запущена.
    discovery: Mutex<Option<CancellationToken>>,
    /// Активная сессия Cloud. Пока её нет, консоль работает в режиме
    /// локального enrollment (ADR-0007).
    cloud: Mutex<Option<cloud::CloudSession>>,
    enrollment: Mutex<EnrollmentAuthority>,
    /// Зарегистрированные устройства. `Arc`, потому что фоновое обнаружение
    /// обновляет их адреса, переживая вызов команды, который его запустил.
    devices: Arc<Mutex<HashMap<String, EnrolledDevice>>>,
    frames: Arc<Mutex<HashMap<String, StoredFrame>>>,
    streams: Arc<Mutex<HashMap<String, CancellationToken>>>,
    remote_controls: Arc<Mutex<HashMap<String, RemoteControlHandle>>>,
}

struct RemoteControlHandle {
    sender: mpsc::UnboundedSender<Envelope>,
    cancellation: CancellationToken,
}

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

#[derive(Debug, Clone, Serialize)]
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
    /// Подробности Repair по каждому приложению. Пусто для остальных команд.
    repair: Vec<RepairItemView>,
}

#[derive(Debug, Serialize)]
struct RepairItemView {
    application_id: String,
    success: bool,
    error_code: String,
}

#[derive(Debug, Serialize)]
struct HealthView {
    device_id: String,
    state: String,
    cpu_percent: f64,
    ram_percent: f64,
    disk_percent: f64,
    os_version: String,
    agent_version: String,
    uptime_seconds: i64,
    profile_id: String,
    /// Машиночитаемые коды; человекочитаемый текст подбирает UI.
    warnings: Vec<String>,
    drift: Vec<DriftView>,
}

#[derive(Debug, Serialize)]
struct DriftView {
    application_id: String,
    kind: String,
    required_version: String,
    actual_version: String,
}

#[derive(Debug, Clone, Serialize)]
struct FrameReady {
    device_id: String,
    sequence: u32,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
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

/// Каталог постоянного состояния консоли.
///
/// Рабочий каталог для этого не годится: он зависит от того, откуда запустили
/// приложение, а вместе с ним «переезжает» ключ издателя — то есть запуск из
/// другого места молча отключал бы весь уже зарегистрированный класс.
/// `CLASSOS_TEACHER_STATE_DIR` оставлен для тестов и переносимых стендов.
fn state_dir() -> PathBuf {
    if let Some(explicit) = std::env::var_os("CLASSOS_TEACHER_STATE_DIR") {
        return PathBuf::from(explicit);
    }
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"));
    base.unwrap_or_else(|| PathBuf::from("."))
        .join("ClassOS")
        .join("TeacherConsole")
}

fn authority_path() -> PathBuf {
    state_dir().join("teacher-authority.key")
}

fn load_authority() -> TeacherAuthority {
    let path = authority_path();
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(secret) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return TeacherAuthority::from_secret(&secret);
        }
    }
    let authority = TeacherAuthority::generate().expect("криптографическая случайность доступна");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, authority.secret_bytes());
    authority
}

#[tauri::command]
fn create_enrollment_code(state: State<'_, AppState>) -> Result<EnrollmentCodeView, String> {
    let mut authority = state.enrollment.lock().map_err(|_| "состояние занято")?;
    let code = authority.issue(
        EnrollmentContext::default(),
        now_ms(),
        DEFAULT_ENROLLMENT_TTL,
    );
    Ok(EnrollmentCodeView {
        code: code.value,
        expires_at_unix_ms: code.expires_at_unix_ms,
    })
}

/// Разбирает hex-строку Cloud в байты.
/// Ждёт ответ устройства на конкретный запрос.
///
/// По тому же соединению устройство присылает heartbeat, статус и
/// периодический health-отчёт, причём первый heartbeat уходит немедленно
/// после авторизации. Читать ровно одно сообщение поэтому нельзя: ответом
/// оказался бы heartbeat, и любая операция консоли завершалась бы «устройство
/// вернуло неожиданный ответ».
///
/// `answer` распознаёт нужное сообщение; всё остальное пропускается. Ожидание
/// ограничено по времени, чтобы молчащее устройство не подвешивало консоль.
async fn recv_answer<S, T>(
    connection: &mut transport::ControlConnection<S>,
    what: &str,
    mut answer: impl FnMut(envelope::Payload) -> Option<T>,
) -> Result<T, String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let wait = async {
        loop {
            let payload = connection
                .recv()
                .await
                .map_err(|error| error.to_string())?
                .and_then(|message| message.payload)
                .ok_or_else(|| format!("устройство закрыло соединение до ответа: {what}"))?;
            if let Some(value) = answer(payload) {
                return Ok(value);
            }
        }
    };
    tokio::time::timeout(RESPONSE_TIMEOUT, wait)
        .await
        .map_err(|_| format!("устройство не ответило вовремя: {what}"))?
}

/// Сколько ждать ответ устройства на запрос.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);

fn bytes_to_hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Версия формата реестра устройств на диске.
const DEVICE_REGISTRY_VERSION: u32 = 1;

fn devices_path() -> PathBuf {
    state_dir().join("enrolled-devices.json")
}

/// Строка реестра. Сертификат целиком не хранится: закреплён отпечаток, а сам
/// сертификат приходит заново с каждым TLS-соединением.
#[derive(Serialize, Deserialize)]
struct StoredDevice {
    device_id: String,
    credential: String,
    fingerprint: String,
    ip: String,
    control_port: u16,
}

#[derive(Serialize, Deserialize)]
struct DeviceRegistry {
    version: u32,
    devices: Vec<StoredDevice>,
}

/// Восстанавливает зарегистрированные устройства с диска.
///
/// Без этого перезапуск консоли терял credential и закреплённый отпечаток, а
/// вернуть их было нечем: устройство, прошедшее enrollment, больше никогда не
/// присылает `EnrollmentRequest`, поэтому повторная регистрация невозможна и
/// класс приходилось бы переустанавливать руками.
///
/// Каждая запись проверяется тем же способом, что и при выдаче: чужой или
/// протухший credential не восстанавливается, а отбрасывается с записью в
/// журнал — молча пропавшее устройство хуже, чем названная причина.
fn load_devices(issuer_public_key: &[u8; 32], now_unix_ms: i64) -> HashMap<String, EnrolledDevice> {
    load_devices_from(&devices_path(), issuer_public_key, now_unix_ms)
}

fn load_devices_from(
    path: &Path,
    issuer_public_key: &[u8; 32],
    now_unix_ms: i64,
) -> HashMap<String, EnrolledDevice> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let registry: DeviceRegistry = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("реестр устройств повреждён ({path:?}): {error}");
            return HashMap::new();
        }
    };
    if registry.version != DEVICE_REGISTRY_VERSION {
        eprintln!(
            "реестр устройств версии {} не поддерживается — ожидалась {}",
            registry.version, DEVICE_REGISTRY_VERSION
        );
        return HashMap::new();
    }

    let mut devices = HashMap::new();
    for stored in registry.devices {
        let Some(encoded) = hex_to_bytes(&stored.credential) else {
            eprintln!("устройство {}: повреждённый credential", stored.device_id);
            continue;
        };
        let Some(fingerprint) = hex_to_bytes(&stored.fingerprint)
            .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
        else {
            eprintln!("устройство {}: повреждённый отпечаток", stored.device_id);
            continue;
        };
        match DeviceCredential::decode_and_verify_fingerprint(
            &encoded,
            issuer_public_key,
            &stored.device_id,
            &fingerprint,
            now_unix_ms,
        ) {
            Ok(credential) => {
                devices.insert(
                    stored.device_id,
                    EnrolledDevice {
                        credential,
                        fingerprint,
                        ip: stored.ip,
                        control_port: stored.control_port,
                    },
                );
            }
            Err(error) => eprintln!(
                "устройство {} не восстановлено: {error}; потребуется повторная регистрация",
                stored.device_id
            ),
        }
    }
    devices
}

/// Записывает реестр целиком: он мал, а частичная запись означала бы
/// устройство, которое нельзя ни использовать, ни зарегистрировать заново.
fn save_devices(devices: &HashMap<String, EnrolledDevice>) {
    save_devices_to(&devices_path(), devices);
}

fn save_devices_to(path: &Path, devices: &HashMap<String, EnrolledDevice>) {
    let registry = DeviceRegistry {
        version: DEVICE_REGISTRY_VERSION,
        devices: devices
            .iter()
            .map(|(device_id, device)| StoredDevice {
                device_id: device_id.clone(),
                credential: bytes_to_hex(&device.credential.encode()),
                fingerprint: bytes_to_hex(&device.fingerprint),
                ip: device.ip.clone(),
                control_port: device.control_port,
            })
            .collect(),
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&registry) {
        Ok(text) => {
            if let Err(error) = std::fs::write(path, text) {
                eprintln!("не удалось сохранить реестр устройств: {error}");
            }
        }
        Err(error) => eprintln!("не удалось сериализовать реестр устройств: {error}"),
    }
}

fn hex_to_bytes(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) || value.is_empty() {
        return None;
    }
    (0..value.len() / 2)
        .map(|index| u8::from_str_radix(value.get(index * 2..index * 2 + 2)?, 16).ok())
        .collect()
}

fn device_view(received: discovery::ReceivedAnnouncement) -> DeviceView {
    // Адрес берётся из источника пакета, а не из объявления: объявление
    // недоверенное и не должно определять, куда пойдёт соединение.
    let ip = received.source.ip().to_string();
    let announcement = received.announcement;
    DeviceView {
        device_id: announcement.device_id,
        hostname: announcement.hostname,
        ip,
        control_port: announcement.control_port,
        room_hint: announcement.room_hint,
        agent_version: announcement.agent_version,
    }
}

/// Однократный поиск: остаётся для быстрой проверки связи с одним устройством.
#[tauri::command]
async fn discover_device() -> Result<DeviceView, String> {
    let received = tokio::time::timeout(
        Duration::from_secs(8),
        discovery::listen_once(Default::default()),
    )
    .await
    .map_err(|_| "время ожидания discovery истекло".to_owned())?
    .map_err(|error| error.to_string())?;
    Ok(device_view(received))
}

/// Включает непрерывное обнаружение класса.
///
/// Устройства объявляют себя независимо и с разбросом по времени, поэтому
/// класс собирается постепенно: каждое новое объявление уходит во фронтенд
/// событием `device-discovered`. Повторный вызов перезапускает прослушивание,
/// а не заводит второе.
#[tauri::command]
fn start_discovery(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    let cancellation = CancellationToken::new();
    {
        let mut running = state.discovery.lock().map_err(|_| "состояние занято")?;
        if let Some(previous) = running.replace(cancellation.clone()) {
            previous.cancel();
        }
    }

    let devices = Arc::clone(&state.devices);
    tokio::spawn(async move {
        let result = discovery::listen_loop(Default::default(), cancellation, |received| {
            let view = device_view(received);
            // Адрес устройства меняется при перезагрузке и продлении аренды
            // DHCP. Без этого обновления сохранённая запись указывала бы на
            // прежний адрес, и устройство было бы недостижимо, оставаясь
            // видимым в списке.
            refresh_device_address(&devices, &view);
            let _ = app.emit("device-discovered", view);
        })
        .await;
        if let Err(error) = result {
            let _ = app.emit(
                "discovery-status",
                format!("Обнаружение остановлено: {error}"),
            );
        }
    });
    Ok(())
}

/// Переносит новый адрес объявления в зарегистрированную запись.
///
/// Объявление остаётся недоверенным: оно может лишь указать, куда идти к уже
/// известному `device_id`, а закреплённый отпечаток сертификата и credential
/// не меняются — подменить устройство подделкой объявления по-прежнему нельзя.
fn refresh_device_address(
    devices: &Arc<Mutex<HashMap<String, EnrolledDevice>>>,
    view: &DeviceView,
) {
    let Ok(port) = u16::try_from(view.control_port) else {
        return;
    };
    let Ok(mut devices) = devices.lock() else {
        return;
    };
    let Some(device) = devices.get_mut(&view.device_id) else {
        return;
    };
    if device.ip == view.ip && device.control_port == port {
        return;
    }
    device.ip = view.ip.clone();
    device.control_port = port;
    save_devices(&devices);
}

/// Останавливает непрерывное обнаружение.
#[tauri::command]
fn stop_discovery(state: State<'_, AppState>) -> Result<(), String> {
    let mut running = state.discovery.lock().map_err(|_| "состояние занято")?;
    if let Some(cancellation) = running.take() {
        cancellation.cancel();
    }
    Ok(())
}

#[tauri::command]
async fn enroll_device(
    state: State<'_, AppState>,
    device_id: String,
    ip: String,
    control_port: u32,
    code: String,
) -> Result<String, String> {
    let port = u16::try_from(control_port).map_err(|_| "некорректный control_port")?;
    let addr = SocketAddr::new(
        ip.parse::<IpAddr>().map_err(|_| "некорректный IP-адрес")?,
        port,
    );
    let client = TlsClient::bootstrap(&device_id).map_err(|error| error.to_string())?;
    let mut connection = client
        .connect(addr)
        .await
        .map_err(|error| error.to_string())?;
    let fingerprint: [u8; 32] = Sha256::digest(
        connection
            .peer_certificate_der()
            .map_err(|error| error.to_string())?,
    )
    .into();
    let hello = connection
        .recv()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "устройство закрыло соединение до DeviceHello".to_owned())?;
    let device_hello = match hello.payload {
        Some(envelope::Payload::DeviceHello(value)) => value,
        _ => return Err("первым сообщением ожидался DeviceHello".to_owned()),
    };
    let request = connection
        .recv()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "устройство закрыло соединение до EnrollmentRequest".to_owned())?;
    let request = match request.payload {
        Some(envelope::Payload::EnrollmentRequest(value)) => value,
        _ => return Err("ожидался EnrollmentRequest".to_owned()),
    };
    if request.device_id != device_id || device_hello.device_id != device_id {
        return Err("идентификатор устройства не совпадает с discovery".to_owned());
    }
    // Кто именно проверяет одноразовый код, зависит от режима: Cloud, если
    // консоль в него вошла, иначе локальная заглушка ADR-0007. Двух
    // авторитетов одновременно быть не должно.
    let cloud_session = state.cloud.lock().map_err(|_| "состояние занято")?.clone();
    let cloud_enrollment = match &cloud_session {
        Some(session) => Some(
            cloud::enroll_device(
                session,
                &code,
                &device_id,
                &device_hello.hostname,
                &request.device_certificate_der,
            )
            .await
            .map_err(|error| error.to_string())?,
        ),
        None => {
            let mut enrollment = state.enrollment.lock().map_err(|_| "состояние занято")?;
            enrollment
                .consume(&code, &EnrollmentContext::default(), now_ms())
                .map_err(|error| error.to_string())?;
            None
        }
    };

    // Устройство переходит в режим обязательной проверки прав только если
    // Cloud вернул и ключ издателя, и кабинет: ключ без кабинета проверить
    // нечем (ADR-0016).
    let (lease_issuer_public_key, room_id) = match &cloud_enrollment {
        Some(enrollment) => {
            let key = hex_to_bytes(&enrollment.lease_issuer_public_key)
                .ok_or("Cloud вернул некорректный ключ издателя lease")?;
            let room = enrollment
                .room_id
                .clone()
                .ok_or("устройство зарегистрировано в Cloud без кабинета")?;
            (key, room)
        }
        None => (Vec::new(), String::new()),
    };
    let expires = now_ms().saturating_add(Duration::from_secs(30 * 24 * 3600).as_millis() as i64);
    let (issued_credential, issuer_public_key) = {
        let authority = state.authority.lock().map_err(|_| "состояние занято")?;
        let credential =
            authority.issue_device_credential(&device_id, &request.device_certificate_der, expires);
        (credential.encode(), authority.public_key().to_vec())
    };
    let issuer_public_key_array: [u8; 32] = issuer_public_key
        .as_slice()
        .try_into()
        .map_err(|_| "некорректный ключ issuer")?;
    let credential = DeviceCredential::decode_and_verify(
        &issued_credential,
        &issuer_public_key_array,
        &device_id,
        &connection
            .peer_certificate_der()
            .map_err(|error| error.to_string())?,
        now_ms(),
    )
    .map_err(|error| error.to_string())?;
    let result = Envelope {
        protocol_version: protocol::network::PROTOCOL_VERSION,
        message_id: format!("enrollment-result-{}", now_ms()),
        timestamp_ms: now_ms(),
        payload: Some(envelope::Payload::EnrollmentResult(EnrollmentResult {
            success: true,
            error_code: EnrollmentErrorCode::Unspecified as i32,
            issued_credential,
            issuer_public_key_der: issuer_public_key,
            expires_at_unix_ms: expires,
            // Локальный enrollment (ADR-0007) не является Cloud и не выдаёт
            // classroom lease: устройство остаётся в прежнем режиме
            // авторизации, и притворяться иначе нельзя.
            lease_issuer_public_key,
            room_id,
        })),
    };
    connection
        .send(&result)
        .await
        .map_err(|error| error.to_string())?;
    {
        let mut devices = state.devices.lock().map_err(|_| "состояние занято")?;
        devices.insert(
            device_id,
            EnrolledDevice {
                credential,
                fingerprint,
                ip,
                control_port: port,
            },
        );
        // Сразу на диск: перезапуск консоли между регистрацией и первой
        // командой не должен требовать обхода класса руками.
        save_devices(&devices);
    }
    Ok("устройство успешно зарегистрировано".to_owned())
}

/// Запрашивает свежий health-отчёт устройства.
///
/// Отчёт приходит и периодически сам, но администратору нужна возможность
/// нажать кнопку и увидеть актуальное состояние, а не ждать следующего тика.
#[tauri::command]
async fn request_health(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<HealthView, String> {
    let (credential, fingerprint, ip, port) = {
        let devices = state.devices.lock().map_err(|_| "состояние занято")?;
        let device = devices
            .get(&device_id)
            .ok_or_else(|| "устройство ещё не зарегистрировано в этой сессии".to_owned())?;
        (
            device.credential.clone(),
            device.fingerprint,
            device.ip.clone(),
            device.control_port,
        )
    };
    let client = TlsClient::pinned(&device_id, fingerprint).map_err(|error| error.to_string())?;
    let mut connection = client
        .connect(SocketAddr::new(
            ip.parse::<IpAddr>().map_err(|_| "некорректный IP-адрес")?,
            port,
        ))
        .await
        .map_err(|error| error.to_string())?;
    connection
        .recv()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "устройство закрыло соединение".to_owned())?;
    let lease = current_lease(&state);
    let teacher_hello = {
        let authority = state.authority.lock().map_err(|_| "состояние занято")?;
        build_teacher_hello(
            &authority,
            &credential,
            format!("teacher-{}", now_ms()),
            format!("hello-{}", now_ms()),
            now_ms(),
            lease.as_ref(),
        )
    };
    connection
        .send(&teacher_hello)
        .await
        .map_err(|error| error.to_string())?;
    connection
        .recv()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "устройство не подтвердило авторизацию".to_owned())?;
    connection
        .send(&protocol::network::Envelope {
            protocol_version: protocol::network::PROTOCOL_VERSION,
            message_id: format!("health-{}", now_ms()),
            timestamp_ms: now_ms(),
            payload: Some(protocol::network::envelope::Payload::HealthRequest(
                protocol::network::HealthRequest {
                    device_id: device_id.clone(),
                },
            )),
        })
        .await
        .map_err(|error| error.to_string())?;

    // Устройство присылает heartbeat и кадры по тому же соединению, поэтому
    // ждём именно отчёт, а не первое попавшееся сообщение.
    let report = recv_answer(
        &mut connection,
        "состояние устройства",
        |payload| match payload {
            envelope::Payload::DeviceHealthReport(report) => Some(report),
            _ => None,
        },
    )
    .await?;
    Ok(to_health_view(report))
}

fn to_health_view(report: protocol::network::DeviceHealthReport) -> HealthView {
    let state = match protocol::network::DeviceHealthState::try_from(report.state) {
        Ok(protocol::network::DeviceHealthState::Healthy) => "healthy",
        Ok(protocol::network::DeviceHealthState::Warning) => "warning",
        Ok(protocol::network::DeviceHealthState::Critical) => "critical",
        _ => "unknown",
    };
    HealthView {
        device_id: report.device_id,
        state: state.to_owned(),
        cpu_percent: report.cpu_percent,
        ram_percent: report.ram_percent,
        disk_percent: report.disk_percent,
        os_version: report.os_version,
        agent_version: report.agent_version,
        uptime_seconds: report.uptime_seconds,
        profile_id: report.profile_id,
        warnings: report.warnings,
        drift: report
            .drift
            .into_iter()
            .map(|entry| DriftView {
                kind: match protocol::network::DriftKind::try_from(entry.kind) {
                    Ok(protocol::network::DriftKind::Missing) => "missing".to_owned(),
                    Ok(protocol::network::DriftKind::VersionMismatch) => {
                        "version_mismatch".to_owned()
                    }
                    _ => "unknown".to_owned(),
                },
                application_id: entry.application_id,
                required_version: entry.required_version,
                actual_version: entry.actual_version,
            })
            .collect(),
    }
}

#[tauri::command]
async fn request_screenshot(
    state: State<'_, AppState>,
    device_id: String,
    display_id: u32,
) -> Result<String, String> {
    let (credential, fingerprint, ip, port) = {
        let devices = state.devices.lock().map_err(|_| "состояние занято")?;
        let device = devices
            .get(&device_id)
            .ok_or_else(|| "устройство ещё не зарегистрировано в этой сессии".to_owned())?;
        (
            device.credential.clone(),
            device.fingerprint,
            device.ip.clone(),
            device.control_port,
        )
    };
    let client = TlsClient::pinned(&device_id, fingerprint).map_err(|error| error.to_string())?;
    let mut connection = client
        .connect(SocketAddr::new(
            ip.parse::<IpAddr>().map_err(|_| "некорректный IP-адрес")?,
            port,
        ))
        .await
        .map_err(|error| error.to_string())?;
    let _device_hello = connection
        .recv()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "устройство закрыло соединение".to_owned())?;
    let lease = current_lease(&state);
    let teacher_hello = {
        let authority = state.authority.lock().map_err(|_| "состояние занято")?;
        build_teacher_hello(
            &authority,
            &credential,
            format!("teacher-{}", now_ms()),
            format!("hello-{}", now_ms()),
            now_ms(),
            lease.as_ref(),
        )
    };
    connection
        .send(&teacher_hello)
        .await
        .map_err(|error| error.to_string())?;
    let _status = connection
        .recv()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "устройство не подтвердило авторизацию".to_owned())?;
    connection
        .send(&protocol::network::Envelope {
            protocol_version: protocol::network::PROTOCOL_VERSION,
            message_id: format!("screenshot-{}", now_ms()),
            timestamp_ms: now_ms(),
            payload: Some(protocol::network::envelope::Payload::ScreenshotRequest(
                protocol::network::ScreenshotRequest {
                    device_id: device_id.clone(),
                    display_id,
                },
            )),
        })
        .await
        .map_err(|error| error.to_string())?;
    enum Shot {
        Frame(Vec<u8>),
        Failed(String),
    }
    let answer = recv_answer(&mut connection, "снимок экрана", |payload| match payload {
        envelope::Payload::ScreenFrame(frame) => Some(Shot::Frame(frame.encoded_data)),
        envelope::Payload::CaptureError(error) => {
            Some(Shot::Failed(format!("{}: {}", error.code, error.message)))
        }
        _ => None,
    })
    .await?;

    match answer {
        Shot::Frame(data) => {
            store_frame(&state.frames, device_id.clone(), data);
            Ok(frame_url(&device_id))
        }
        Shot::Failed(message) => Err(message),
    }
}

#[tauri::command]
async fn start_stream(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    device_id: String,
    selected: bool,
) -> Result<String, String> {
    let lease = current_lease(&state);
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
                lease.as_ref(),
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
                        match message.payload {
                            Some(protocol::network::envelope::Payload::ScreenFrame(frame)) => {
                                let sequence = frame.sequence;
                                store_frame(&frames, task_device_id.clone(), frame.encoded_data);
                                let _ = app.emit("stream-frame-ready", FrameReady { device_id: task_device_id.clone(), sequence });
                            }
                            // Сбой захвата обязан дойти до преподавателя
                            // словами (spec T2 §13.5): молча оставленная
                            // застывшая картинка выглядит как работающий
                            // поток и хуже честной ошибки.
                            Some(protocol::network::envelope::Payload::CaptureError(error)) => {
                                let _ = app.emit("stream-status", format!("{task_device_id}: не удалось получить экран ({}: {})", error.code, error.message));
                            }
                            _ => {}
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
    // Кадр удаляется вместе с потоком: показывать последний снимок
    // остановленного устройства как живой экран нельзя (инвариант 7 —
    // экраны эфемерны).
    if let Ok(mut frames) = state.frames.lock() {
        frames.remove(&device_id);
    }
}

/// Профили урока, которые преподаватель видит как кнопки.
///
/// Teacher Console оперирует только продуктовыми понятиями: имя профиля и
/// идентификаторы приложений. Ни AppLocker, ни registry, ни SID сюда не
/// попадают — это инвариант X (`CLAUDE.md`), а не вопрос вкуса.
fn lesson_profile(profile_id: &str) -> Option<policy_engine::LessonPolicy> {
    let build = |name: &str, applications: &[&str], urls: &[&str]| policy_engine::LessonPolicy {
        name: name.to_owned(),
        allowed_applications: applications.iter().map(|v| (*v).to_owned()).collect(),
        allowed_urls: urls.iter().map(|v| (*v).to_owned()).collect(),
        block_settings: true,
        block_powershell: true,
        block_cmd: true,
        block_store: true,
        block_personalization: true,
        restrict_to_allowed: false,
    };
    match profile_id {
        "python" => Some(build(
            "Python",
            &["vscode", "python", "chrome"],
            &["docs.python.org", "github.com"],
        )),
        "web" => Some(build(
            "Web",
            &["vscode", "chrome"],
            &["developer.mozilla.org", "github.com"],
        )),
        _ => None,
    }
}

fn command_body(
    kind: &str,
    value: &str,
    device_id: &str,
) -> Result<protocol::network::command::Body, String> {
    use protocol::network::command::Body;
    match kind {
        "policy" => {
            let profile =
                lesson_profile(value).ok_or_else(|| "неизвестный профиль урока".to_owned())?;
            let document = policy_engine::PolicyDocument::new(profile)
                .encode()
                .map_err(|error| error.to_string())?;
            Ok(Body::ApplyPolicy(protocol::network::ApplyPolicy {
                policy_id: value.to_owned(),
                compiled_policy: document,
            }))
        }
        // Пустой snapshot_id означает "снять активную политику устройства":
        // Teacher Console не хранит и не должна знать идентификаторы снимков.
        "policy_off" => Ok(Body::RollbackPolicy(protocol::network::RollbackPolicy {
            snapshot_id: String::new(),
        })),
        "focus" if !value.is_empty() => {
            Ok(Body::FocusModeEnable(protocol::network::FocusModeEnable {
                allowed_application_ids: value
                    .split(',')
                    .map(|id| id.trim().to_owned())
                    .filter(|id| !id.is_empty())
                    .collect(),
            }))
        }
        "focus_off" => Ok(Body::FocusModeDisable(
            protocol::network::FocusModeDisable {},
        )),
        "repair" if !value.is_empty() => Ok(Body::RepairDesiredState(
            protocol::network::RepairDesiredState {
                device_id: device_id.to_owned(),
                profile_id: value.to_owned(),
            },
        )),
        "lock" => Ok(Body::LockDevice(protocol::network::LockDevice {})),
        "unlock" => Ok(Body::UnlockDevice(protocol::network::UnlockDevice {})),
        "message" if !value.is_empty() => Ok(Body::ShowMessage(protocol::network::ShowMessage {
            text: value.to_owned(),
        })),
        "application" => Ok(Body::LaunchApplication(
            protocol::network::LaunchApplication {
                application_id: value.to_owned(),
            },
        )),
        "url" => Ok(Body::OpenUrl(protocol::network::OpenUrl {
            url: value.to_owned(),
        })),
        "restart" => Ok(Body::RestartDevice(protocol::network::RestartDevice {})),
        "shutdown" => Ok(Body::ShutdownDevice(protocol::network::ShutdownDevice {})),
        _ => Err("неизвестная или неполная classroom-команда".to_owned()),
    }
}

async fn dispatch_command_to_device(
    device_id: String,
    device: EnrolledDevice,
    authority_secret: [u8; 32],
    lease: Option<SignedLease>,
    kind: String,
    value: String,
) -> CommandResultView {
    let command_id = Uuid::new_v4().to_string();
    let result: Result<(protocol::network::CommandResult, Vec<RepairItemView>), String> = async {
        let body = command_body(&kind, &value, &device_id)?;
        let client =
            TlsClient::pinned(&device_id, device.fingerprint).map_err(|error| error.to_string())?;
        let address = SocketAddr::new(
            device
                .ip
                .parse::<IpAddr>()
                .map_err(|_| "некорректный IP-адрес".to_owned())?,
            device.control_port,
        );
        let mut connection = client
            .connect(address)
            .await
            .map_err(|error| error.to_string())?;
        connection
            .recv()
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "устройство закрыло соединение".to_owned())?;
        let authority = TeacherAuthority::from_secret(&authority_secret);
        connection
            .send(&build_teacher_hello(
                &authority,
                &device.credential,
                format!("teacher-{}", now_ms()),
                format!("hello-{}", now_ms()),
                now_ms(),
                lease.as_ref(),
            ))
            .await
            .map_err(|error| error.to_string())?;
        connection
            .recv()
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "устройство не подтвердило авторизацию".to_owned())?;
        connection
            .send(&Envelope {
                protocol_version: protocol::network::PROTOCOL_VERSION,
                message_id: format!("command-{}", command_id),
                timestamp_ms: now_ms(),
                payload: Some(envelope::Payload::Command(protocol::network::Command {
                    command_id: command_id.clone(),
                    expires_at_unix_ms: now_ms().saturating_add(30_000),
                    body: Some(body),
                })),
            })
            .await
            .map_err(|error| error.to_string())?;
        // Repair присылает подробности по каждому приложению отдельным
        // сообщением **до** результата команды, поэтому читать ровно одно
        // сообщение нельзя: иначе успешный Repair выглядел бы как
        // неожиданный ответ устройства (spec T7 §8).
        let mut repair_items = Vec::new();
        let result = recv_answer(
            &mut connection,
            "результат команды",
            |payload| match payload {
                envelope::Payload::CommandResult(result) => Some(result),
                envelope::Payload::RepairResult(result) => {
                    repair_items = result
                        .items
                        .into_iter()
                        .map(|item| RepairItemView {
                            application_id: item.application_id,
                            success: item.success,
                            error_code: item.error_code,
                        })
                        .collect();
                    None
                }
                _ => None,
            },
        )
        .await?;
        Ok((result, repair_items))
    }
    .await;
    match result {
        Ok((result, repair)) => CommandResultView {
            device_id,
            command_id: result.command_id,
            success: result.success,
            error_code: result.error_code,
            message: result.message,
            repair,
        },
        Err(message) => CommandResultView {
            device_id,
            command_id,
            success: false,
            error_code: "COMMAND_DELIVERY_FAILED".to_owned(),
            message,
            repair: Vec::new(),
        },
    }
}

#[tauri::command]
async fn dispatch_classroom_command(
    state: State<'_, AppState>,
    device_ids: Vec<String>,
    kind: String,
    value: String,
) -> Result<Vec<CommandResultView>, String> {
    if device_ids.is_empty() {
        return Err("не выбраны устройства".to_owned());
    }
    // Ранняя проверка формы команды, чтобы не рассылать заведомо неверную.
    command_body(&kind, &value, "validation")?;
    let lease = current_lease(&state);
    let (devices, authority_secret) = {
        let stored = state.devices.lock().map_err(|_| "состояние занято")?;
        let devices = device_ids
            .into_iter()
            .filter_map(|id| stored.get(&id).cloned().map(|device| (id, device)))
            .collect::<Vec<_>>();
        let authority = state.authority.lock().map_err(|_| "состояние занято")?;
        (devices, authority.secret_bytes())
    };
    let mut tasks = JoinSet::new();
    for (device_id, device) in devices {
        tasks.spawn(dispatch_command_to_device(
            device_id,
            device,
            authority_secret,
            lease.clone(),
            kind.clone(),
            value.clone(),
        ));
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
        let device = devices
            .get(&device_id)
            .ok_or_else(|| "устройство ещё не зарегистрировано".to_owned())?;
        let authority = state.authority.lock().map_err(|_| "состояние занято")?;
        (
            device.credential.clone(),
            device.fingerprint,
            device.ip.clone(),
            device.control_port,
            authority.secret_bytes(),
        )
    };
    let lease = current_lease(&state);
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
            connection.send(&build_teacher_hello(&authority, &credential, format!("teacher-{}", now_ms()), format!("hello-{}", now_ms()), now_ms(), lease.as_ref())).await.map_err(|error| error.to_string())?;
            connection.recv().await.map_err(|error| error.to_string())?.ok_or_else(|| "устройство не подтвердило авторизацию".to_owned())?;
            connection.send(&Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("remote-start-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(envelope::Payload::RemoteControlStart(protocol::network::RemoteControlStart { device_id: task_device_id.clone() })) }).await.map_err(|error| error.to_string())?;
            // Heartbeat устройства приходит по этому же соединению и уходит
            // сразу после авторизации, поэтому ждём именно ответ на старт.
            let started = recv_answer(&mut connection, "старт удалённого управления", |payload| match payload {
                envelope::Payload::RemoteControlStarted(_) => Some(Ok(())),
                envelope::Payload::RemoteControlStopped(value) => {
                    Some(Err(format!("remote control отклонён: {}", value.reason)))
                }
                _ => None,
            })
            .await?;
            if let Err(reason) = started {
                if let Some(sender) = ready_tx.take() { let _ = sender.send(Err(reason)); }
                return Ok(());
            }
            if let Some(sender) = ready_tx.take() { let _ = sender.send(Ok(())); }
            let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
            loop { tokio::select! {
                _ = task_cancellation.cancelled() => { let _ = connection.send(&Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("remote-stop-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(envelope::Payload::RemoteControlStop(protocol::network::RemoteControlStop { device_id: task_device_id.clone() })) }).await; return Ok(()); }
                _ = heartbeat.tick() => connection.send(&Envelope { protocol_version: protocol::network::PROTOCOL_VERSION, message_id: format!("heartbeat-{}", now_ms()), timestamp_ms: now_ms(), payload: Some(envelope::Payload::Heartbeat(protocol::network::Heartbeat { sequence: now_ms() as u64, sent_at_unix_ms: now_ms() })) }).await.map_err(|error| error.to_string())?,
                message = receiver.recv() => match message { Some(message) => connection.send(&message).await.map_err(|error| error.to_string())?, None => return Ok(()) },
                received = connection.recv() => { if received.map_err(|error| error.to_string())?.is_none() { return Ok(()); } }
            }}
        }.await;
        if let Err(error) = result {
            if let Some(sender) = ready_tx.take() {
                let _ = sender.send(Err(error));
            }
        }
    });
    tokio::time::timeout(Duration::from_secs(4), ready_rx)
        .await
        .map_err(|_| "время ожидания remote control истекло".to_owned())?
        .map_err(|_| "remote control task завершилась".to_owned())??;
    state
        .remote_controls
        .lock()
        .map_err(|_| "состояние занято")?
        .insert(
            device_id,
            RemoteControlHandle {
                sender,
                cancellation,
            },
        );
    Ok(())
}

#[tauri::command]
fn stop_remote_control(state: State<'_, AppState>, device_id: String) {
    if let Ok(mut controls) = state.remote_controls.lock() {
        if let Some(handle) = controls.remove(&device_id) {
            handle.cancellation.cancel();
        }
    }
}

#[tauri::command]
fn send_remote_mouse_move(
    state: State<'_, AppState>,
    device_id: String,
    x: f32,
    y: f32,
) -> Result<(), String> {
    send_remote_event(
        &state,
        device_id,
        protocol::network::remote_input_event::Event::MouseMove(protocol::network::MouseMove {
            x,
            y,
        }),
    )
}

fn send_remote_event(
    state: &AppState,
    device_id: String,
    event: protocol::network::remote_input_event::Event,
) -> Result<(), String> {
    let handle = state
        .remote_controls
        .lock()
        .map_err(|_| "состояние занято")?
        .get(&device_id)
        .map(|value| value.sender.clone())
        .ok_or_else(|| "remote control не активен".to_owned())?;
    handle
        .send(Envelope {
            protocol_version: protocol::network::PROTOCOL_VERSION,
            message_id: format!("input-{}", now_ms()),
            timestamp_ms: now_ms(),
            payload: Some(envelope::Payload::RemoteInputEvent(
                protocol::network::RemoteInputEvent {
                    device_id,
                    event: Some(event),
                },
            )),
        })
        .map_err(|_| "control-соединение закрыто".to_owned())
}

#[tauri::command]
fn send_remote_mouse_button(
    state: State<'_, AppState>,
    device_id: String,
    button: i32,
    is_down: bool,
    x: f32,
    y: f32,
) -> Result<(), String> {
    send_remote_event(
        &state,
        device_id,
        protocol::network::remote_input_event::Event::MouseButton(protocol::network::MouseButton {
            button,
            is_down,
            x,
            y,
        }),
    )
}
#[tauri::command]
fn send_remote_wheel(
    state: State<'_, AppState>,
    device_id: String,
    delta: i32,
) -> Result<(), String> {
    send_remote_event(
        &state,
        device_id,
        protocol::network::remote_input_event::Event::MouseWheel(protocol::network::MouseWheel {
            delta,
        }),
    )
}
#[tauri::command]
fn send_remote_key(
    state: State<'_, AppState>,
    device_id: String,
    virtual_key_code: u32,
    is_down: bool,
) -> Result<(), String> {
    send_remote_event(
        &state,
        device_id,
        protocol::network::remote_input_event::Event::KeyEvent(protocol::network::KeyEvent {
            virtual_key_code,
            is_down,
        }),
    )
}

/// Текущий lease преподавателя, если Cloud его выдал.
///
/// Отсутствие lease — не ошибка на стороне консоли: решение о том, обязателен
/// ли он, принимает устройство (ADR-0016).
/// Вход в Cloud.
///
/// До входа консоль остаётся полностью работоспособной для устройств из
/// локального enrollment: Cloud добавляет разграничение прав, а не
/// возможность вести урок (инвариант 5).
#[tauri::command]
async fn cloud_sign_in(
    state: State<'_, AppState>,
    base_url: String,
    email: String,
    password: String,
) -> Result<Vec<cloud::MembershipView>, String> {
    let (session, memberships) = cloud::sign_in(&base_url, &email, &password)
        .await
        .map_err(|error| error.to_string())?;
    *state.cloud.lock().map_err(|_| "состояние занято")? = Some(session);
    Ok(memberships)
}

/// Выход из Cloud: сессия и lease забываются вместе.
#[tauri::command]
fn cloud_sign_out(state: State<'_, AppState>) -> Result<(), String> {
    *state.cloud.lock().map_err(|_| "состояние занято")? = None;
    *state.lease.lock().map_err(|_| "состояние занято")? = None;
    Ok(())
}

/// Получает classroom lease на филиал и запоминает его на время урока.
#[tauri::command]
async fn cloud_issue_lease(
    state: State<'_, AppState>,
    organization_id: String,
    branch_id: String,
) -> Result<i64, String> {
    let session = current_cloud_session(&state)?;
    let (lease, _issuer) = cloud::issue_lease(&session, &organization_id, &branch_id)
        .await
        .map_err(|error| error.to_string())?;
    let expires_at = lease.lease.expires_at_unix_ms;
    *state.lease.lock().map_err(|_| "состояние занято")? = Some(lease);
    Ok(expires_at)
}

/// Выпускает enrollment-код в Cloud, а не локально.
#[tauri::command]
async fn cloud_create_enrollment_code(
    state: State<'_, AppState>,
    branch_id: String,
    room_id: Option<String>,
) -> Result<EnrollmentCodeView, String> {
    let session = current_cloud_session(&state)?;
    let code = cloud::create_enrollment_code(&session, &branch_id, room_id.as_deref())
        .await
        .map_err(|error| error.to_string())?;
    Ok(EnrollmentCodeView {
        code: code.code,
        expires_at_unix_ms: code.expires_at_unix_ms,
    })
}

fn current_cloud_session(state: &AppState) -> Result<cloud::CloudSession, String> {
    state
        .cloud
        .lock()
        .map_err(|_| "состояние занято")?
        .clone()
        .ok_or_else(|| "нет активной сессии Cloud".to_owned())
}

fn current_lease(state: &AppState) -> Option<SignedLease> {
    state.lease.lock().ok().and_then(|value| value.clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Явный выбор провайдера rustls до первого TLS-соединения: полагаться на
    // автоматический выбор нельзя, он ломается от любой новой зависимости с
    // другим провайдером.
    transport::install_crypto_provider();

    let frames = Arc::new(Mutex::new(HashMap::new()));
    let stream_frames = Arc::clone(&frames);
    // Ключ издателя восстанавливается первым: реестр устройств проверяется
    // именно им, и без совпадения ключа ни одна запись не годится.
    let authority = load_authority();
    let devices = load_devices(&authority.public_key(), now_ms());
    tauri::Builder::default()
        .manage(AppState {
            authority: Mutex::new(authority),
            lease: Mutex::new(None),
            discovery: Mutex::new(None),
            cloud: Mutex::new(None),
            enrollment: Mutex::new(EnrollmentAuthority::default()),
            devices: Arc::new(Mutex::new(devices)),
            frames,
            streams: Arc::new(Mutex::new(HashMap::new())),
            remote_controls: Arc::new(Mutex::new(HashMap::new())),
        })
        .register_uri_scheme_protocol("classos-frame", move |_, request| {
            let device_id = request
                .uri()
                .path()
                .strip_prefix("/frames/")
                .unwrap_or_default();
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
            start_discovery,
            stop_discovery,
            cloud_sign_in,
            cloud_sign_out,
            cloud_issue_lease,
            cloud_create_enrollment_code,
            enroll_device,
            request_screenshot,
            request_health,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("classos-{name}-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn enrolled(authority: &TeacherAuthority, device_id: &str, expires: i64) -> EnrolledDevice {
        let certificate = format!("certificate-of-{device_id}");
        let credential =
            authority.issue_device_credential(device_id, certificate.as_bytes(), expires);
        EnrolledDevice {
            fingerprint: Sha256::digest(certificate.as_bytes()).into(),
            credential,
            ip: "192.168.1.10".to_owned(),
            control_port: 45901,
        }
    }

    /// Перезапуск консоли обязан сохранять доступ к классу: устройство,
    /// прошедшее enrollment, второй раз его не проходит, и потерянный
    /// credential означал бы обход всех машин руками.
    #[test]
    fn registry_survives_restart() {
        let authority = TeacherAuthority::generate().unwrap();
        let path = temp_path("registry-restart");
        let mut devices = HashMap::new();
        devices.insert(
            "device-1".to_owned(),
            enrolled(&authority, "device-1", 10_000),
        );
        save_devices_to(&path, &devices);

        let restored = load_devices_from(&path, &authority.public_key(), 5_000);
        assert_eq!(restored.len(), 1);
        let device = restored.get("device-1").unwrap();
        assert_eq!(device.control_port, 45901);
        assert_eq!(device.fingerprint, devices["device-1"].fingerprint);
        assert_eq!(
            device.credential.encode(),
            devices["device-1"].credential.encode()
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Ключ издателя сменился — значит записи выпущены не этой консолью и
    /// подключиться по ним всё равно не выйдет.
    #[test]
    fn registry_rejects_credentials_of_another_issuer() {
        let authority = TeacherAuthority::generate().unwrap();
        let stranger = TeacherAuthority::generate().unwrap();
        let path = temp_path("registry-issuer");
        let mut devices = HashMap::new();
        devices.insert(
            "device-1".to_owned(),
            enrolled(&authority, "device-1", 10_000),
        );
        save_devices_to(&path, &devices);

        assert!(load_devices_from(&path, &stranger.public_key(), 5_000).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn registry_drops_expired_credentials() {
        let authority = TeacherAuthority::generate().unwrap();
        let path = temp_path("registry-expired");
        let mut devices = HashMap::new();
        devices.insert(
            "device-1".to_owned(),
            enrolled(&authority, "device-1", 1_000),
        );
        save_devices_to(&path, &devices);

        assert!(load_devices_from(&path, &authority.public_key(), 5_000).is_empty());
        let _ = std::fs::remove_file(&path);
    }

    /// Устройство сменило адрес после перезагрузки: запись обязана указывать
    /// на новый адрес, иначе устройство видно в списке, но недостижимо.
    #[test]
    fn discovery_refreshes_stored_address() {
        let authority = TeacherAuthority::generate().unwrap();
        let mut initial = HashMap::new();
        initial.insert(
            "device-1".to_owned(),
            enrolled(&authority, "device-1", 10_000),
        );
        let devices = Arc::new(Mutex::new(initial));

        refresh_device_address(
            &devices,
            &DeviceView {
                device_id: "device-1".to_owned(),
                hostname: "PC-01".to_owned(),
                ip: "192.168.1.55".to_owned(),
                control_port: 45901,
                room_hint: String::new(),
                agent_version: "0.1.0".to_owned(),
            },
        );

        let devices = devices.lock().unwrap();
        assert_eq!(devices["device-1"].ip, "192.168.1.55");
    }

    /// Неизвестное устройство не появляется в реестре от одного объявления:
    /// discovery остаётся недоверенным каналом.
    #[test]
    fn discovery_never_adds_unknown_device() {
        let devices = Arc::new(Mutex::new(HashMap::new()));
        refresh_device_address(
            &devices,
            &DeviceView {
                device_id: "unknown".to_owned(),
                hostname: "PC-99".to_owned(),
                ip: "192.168.1.99".to_owned(),
                control_port: 45901,
                room_hint: String::new(),
                agent_version: "0.1.0".to_owned(),
            },
        );
        assert!(devices.lock().unwrap().is_empty());
    }
}
