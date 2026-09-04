//! Клиент Cloud v0 для Teacher Console (spec T8 §5–§7).
//!
//! Консоль — единственная сторона, у которой есть сессия Cloud: устройство в
//! Cloud не ходит и получает всё необходимое через уже существующий
//! enrollment-обмен, формат которого T8 §6 запрещает менять.
//!
//! Здесь нет ни одного решения о правах: их принимает Cloud и запечатывает в
//! подписанном lease, а проверяет устройство (ADR-0016).

use serde::{Deserialize, Serialize};
use transport::{ClassroomLease, Permission, SignedLease};

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("Cloud недоступен: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Cloud отклонил запрос: {code} (HTTP {status})")]
    Rejected { status: u16, code: String },
    #[error("Cloud вернул некорректный classroom lease: {0}")]
    MalformedLease(&'static str),
}

/// Активная сессия Cloud.
#[derive(Debug, Clone)]
pub struct CloudSession {
    pub base_url: String,
    pub token: String,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipView {
    #[serde(rename = "organizationId")]
    pub organization_id: String,
    #[serde(rename = "branchId")]
    pub branch_id: Option<String>,
    pub role: String,
}

#[derive(Debug, Deserialize)]
struct MeResponse {
    memberships: Vec<MembershipView>,
}

#[derive(Debug, Deserialize)]
struct LeaseBody {
    #[serde(rename = "teacherId")]
    teacher_id: String,
    #[serde(rename = "organizationId")]
    organization_id: String,
    #[serde(rename = "branchId")]
    branch_id: String,
    #[serde(rename = "allowedRooms")]
    allowed_rooms: Vec<String>,
    permissions: Vec<String>,
    #[serde(rename = "issuedAtUnixMs")]
    issued_at_unix_ms: i64,
    #[serde(rename = "expiresAtUnixMs")]
    expires_at_unix_ms: i64,
}

#[derive(Debug, Deserialize)]
struct LeaseResponse {
    lease: LeaseBody,
    signature: String,
    issuer_public_key: String,
}

#[derive(Debug, Deserialize)]
pub struct EnrollmentCodeResponse {
    pub code: String,
    pub expires_at_unix_ms: i64,
}

/// Ответ Cloud на регистрацию устройства.
#[derive(Debug, Deserialize)]
pub struct CloudEnrollment {
    pub room_id: Option<String>,
    pub lease_issuer_public_key: String,
}

fn hex_to_array(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(value.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(bytes)
}

fn hex_to_signature(value: &str) -> Option<[u8; 64]> {
    if value.len() != 128 {
        return None;
    }
    let mut bytes = [0_u8; 64];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(value.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(bytes)
}

async fn ensure_ok(response: reqwest::Response) -> Result<reqwest::Response, CloudError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status().as_u16();
    // Код ошибки Cloud информативнее HTTP-статуса: FORBIDDEN и
    // INVALID_CREDENTIALS требуют от преподавателя разных действий.
    let code = response
        .json::<ErrorBody>()
        .await
        .ok()
        .and_then(|body| body.error)
        .unwrap_or_else(|| "UNKNOWN".to_owned());
    Err(CloudError::Rejected { status, code })
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}{path}", base_url.trim_end_matches('/'))
}

/// Вход в Cloud. Возвращает сессию и членства пользователя.
pub async fn sign_in(
    base_url: &str,
    email: &str,
    password: &str,
) -> Result<(CloudSession, Vec<MembershipView>), CloudError> {
    let client = reqwest::Client::new();
    let response = ensure_ok(
        client
            .post(endpoint(base_url, "/v1/auth/login"))
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await?,
    )
    .await?;
    let session = CloudSession {
        base_url: base_url.trim_end_matches('/').to_owned(),
        token: response.json::<LoginResponse>().await?.token,
    };

    let me = ensure_ok(
        client
            .get(endpoint(&session.base_url, "/v1/me"))
            .bearer_auth(&session.token)
            .send()
            .await?,
    )
    .await?;
    Ok((session, me.json::<MeResponse>().await?.memberships))
}

/// Запрашивает classroom lease на филиал.
///
/// Права не выбираются здесь: Cloud выдаёт ровно те, что даёт роль (§12.5).
/// Неизвестное имя права — ошибка, а не «пропустим»: молча урезанный lease
/// выглядел бы как отозванные права.
pub async fn issue_lease(
    session: &CloudSession,
    organization_id: &str,
    branch_id: &str,
) -> Result<(SignedLease, [u8; 32]), CloudError> {
    let response = ensure_ok(
        reqwest::Client::new()
            .post(endpoint(&session.base_url, "/v1/lease"))
            .bearer_auth(&session.token)
            .json(&serde_json::json!({
                "organization_id": organization_id,
                "branch_id": branch_id,
            }))
            .send()
            .await?,
    )
    .await?;
    let body = response.json::<LeaseResponse>().await?;

    let mut permissions = Vec::with_capacity(body.lease.permissions.len());
    for name in &body.lease.permissions {
        permissions.push(
            Permission::parse(name)
                .ok_or(CloudError::MalformedLease("неизвестное право в lease"))?,
        );
    }
    let signature = hex_to_signature(&body.signature)
        .ok_or(CloudError::MalformedLease("некорректная подпись"))?;
    let issuer = hex_to_array(&body.issuer_public_key)
        .ok_or(CloudError::MalformedLease("некорректный ключ издателя"))?;

    Ok((
        SignedLease {
            lease: ClassroomLease {
                teacher_id: body.lease.teacher_id,
                organization_id: body.lease.organization_id,
                branch_id: body.lease.branch_id,
                allowed_rooms: body.lease.allowed_rooms,
                permissions,
                issued_at_unix_ms: body.lease.issued_at_unix_ms,
                expires_at_unix_ms: body.lease.expires_at_unix_ms,
            },
            signature,
        },
        issuer,
    ))
}

/// Выпускает одноразовый enrollment-код в Cloud.
pub async fn create_enrollment_code(
    session: &CloudSession,
    branch_id: &str,
    room_id: Option<&str>,
) -> Result<EnrollmentCodeResponse, CloudError> {
    let response = ensure_ok(
        reqwest::Client::new()
            .post(endpoint(&session.base_url, "/v1/enrollment/codes"))
            .bearer_auth(&session.token)
            .json(&serde_json::json!({ "branch_id": branch_id, "room_id": room_id }))
            .send()
            .await?,
    )
    .await?;
    Ok(response.json().await?)
}

/// Регистрирует устройство в Cloud по одноразовому коду.
///
/// Передаётся только публичный сертификат: приватный ключ остаётся на
/// устройстве (инвариант T8 §12.1).
pub async fn enroll_device(
    session: &CloudSession,
    code: &str,
    device_id: &str,
    hostname: &str,
    certificate_der: &[u8],
) -> Result<CloudEnrollment, CloudError> {
    use base64::Engine;

    let response = ensure_ok(
        reqwest::Client::new()
            .post(endpoint(&session.base_url, "/v1/enrollment/enroll"))
            .json(&serde_json::json!({
                "code": code,
                "device_id": device_id,
                "hostname": hostname,
                "certificate_der_base64":
                    base64::engine::general_purpose::STANDARD.encode(certificate_der),
            }))
            .send()
            .await?,
    )
    .await?;
    Ok(response.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_tolerates_trailing_slash() {
        assert_eq!(
            endpoint("https://cloud.example.org/", "/v1/lease"),
            "https://cloud.example.org/v1/lease"
        );
    }

    #[test]
    fn hex_values_must_have_exact_length() {
        assert!(hex_to_array(&"ab".repeat(32)).is_some());
        assert!(hex_to_array(&"ab".repeat(31)).is_none());
        assert!(hex_to_signature(&"ab".repeat(64)).is_some());
        assert!(hex_to_signature("").is_none());
    }

    /// Нешестнадцатеричная строка нужной длины не должна превращаться в ключ.
    #[test]
    fn non_hex_is_not_accepted() {
        assert!(hex_to_array(&"zz".repeat(32)).is_none());
    }
}
