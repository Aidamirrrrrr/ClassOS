//! Построение и проверка handshake Teacher ↔ Agent.

use protocol::network::envelope::Payload;
use protocol::network::{
    DeviceHello, Envelope, PROTOCOL_VERSION, TeacherHello, UpgradeRequired, negotiate_version,
};

use crate::{AuthorizationError, DeviceCredential, TeacherAuthority};

/// Максимальное отклонение времени подписанного TeacherHello от времени Agent.
pub const HELLO_CLOCK_SKEW_MS: i64 = 30_000;

#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
    #[error("ожидался TeacherHello")]
    ExpectedTeacherHello,
    #[error("TeacherHello имеет пустой message_id")]
    MissingMessageId,
    #[error("timestamp TeacherHello находится вне допустимого окна")]
    StaleTimestamp,
    #[error("авторизация TeacherHello отклонена: {0}")]
    Authorization(#[from] AuthorizationError),
    #[error("диапазоны версий протокола не пересекаются")]
    UpgradeRequired(UpgradeRequired),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedTeacher {
    pub teacher_session_id: String,
    pub negotiated_protocol: u32,
}

pub fn build_device_hello(
    message_id: String,
    timestamp_ms: i64,
    device_id: String,
    hostname: String,
    agent_version: String,
    os_version: String,
) -> Envelope {
    Envelope {
        protocol_version: PROTOCOL_VERSION,
        message_id,
        timestamp_ms,
        payload: Some(Payload::DeviceHello(DeviceHello {
            device_id,
            hostname,
            agent_version,
            os_version,
            capabilities: vec!["device_status".to_owned(), "heartbeat".to_owned()],
            min_protocol: PROTOCOL_VERSION,
            max_protocol: PROTOCOL_VERSION,
        })),
    }
}

pub fn build_teacher_hello(
    authority: &TeacherAuthority,
    credential: &DeviceCredential,
    teacher_session_id: String,
    message_id: String,
    timestamp_ms: i64,
) -> Envelope {
    let signature = authority.sign_teacher_hello(
        &teacher_session_id,
        PROTOCOL_VERSION,
        PROTOCOL_VERSION,
        &message_id,
        timestamp_ms,
    );
    Envelope {
        protocol_version: PROTOCOL_VERSION,
        message_id,
        timestamp_ms,
        payload: Some(Payload::TeacherHello(TeacherHello {
            teacher_session_id,
            min_protocol: PROTOCOL_VERSION,
            max_protocol: PROTOCOL_VERSION,
            authorization_credential: credential.encode(),
            signature: signature.to_vec(),
        })),
    }
}

pub fn verify_teacher_hello(
    envelope: &Envelope,
    issuer_public_key: &[u8; 32],
    expected_device_id: &str,
    expected_certificate_der: &[u8],
    now_unix_ms: i64,
) -> Result<VerifiedTeacher, HandshakeError> {
    if envelope.message_id.is_empty() {
        return Err(HandshakeError::MissingMessageId);
    }
    if now_unix_ms.abs_diff(envelope.timestamp_ms) > HELLO_CLOCK_SKEW_MS as u64 {
        return Err(HandshakeError::StaleTimestamp);
    }
    let Some(Payload::TeacherHello(hello)) = &envelope.payload else {
        return Err(HandshakeError::ExpectedTeacherHello);
    };
    let Some(version) = negotiate_version(
        PROTOCOL_VERSION,
        PROTOCOL_VERSION,
        hello.min_protocol,
        hello.max_protocol,
    ) else {
        return Err(HandshakeError::UpgradeRequired(UpgradeRequired {
            min_protocol: PROTOCOL_VERSION,
            max_protocol: PROTOCOL_VERSION,
        }));
    };
    let credential = DeviceCredential::decode_and_verify(
        &hello.authorization_credential,
        issuer_public_key,
        expected_device_id,
        expected_certificate_der,
        now_unix_ms,
    )?;
    DeviceCredential::verify_teacher_hello(
        issuer_public_key,
        &hello.teacher_session_id,
        hello.min_protocol,
        hello.max_protocol,
        &envelope.message_id,
        envelope.timestamp_ms,
        &hello.signature,
    )?;
    drop(credential);
    Ok(VerifiedTeacher {
        teacher_session_id: hello.teacher_session_id.clone(),
        negotiated_protocol: version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_teacher_hello_is_verified() {
        let authority = TeacherAuthority::generate().unwrap();
        let cert = b"device-certificate";
        let credential = authority.issue_device_credential("device-1", cert, 100_000);
        let hello = build_teacher_hello(
            &authority,
            &credential,
            "teacher-1".to_owned(),
            "message-1".to_owned(),
            50_000,
        );
        let verified =
            verify_teacher_hello(&hello, &authority.public_key(), "device-1", cert, 50_001)
                .unwrap();
        assert_eq!(verified.teacher_session_id, "teacher-1");
        assert_eq!(verified.negotiated_protocol, PROTOCOL_VERSION);
    }

    #[test]
    fn stale_or_modified_teacher_hello_is_rejected() {
        let authority = TeacherAuthority::generate().unwrap();
        let cert = b"device-certificate";
        let credential = authority.issue_device_credential("device-1", cert, 100_000);
        let mut hello = build_teacher_hello(
            &authority,
            &credential,
            "teacher-1".to_owned(),
            "message-1".to_owned(),
            50_000,
        );
        assert!(matches!(
            verify_teacher_hello(&hello, &authority.public_key(), "device-1", cert, 90_001,),
            Err(HandshakeError::StaleTimestamp)
        ));
        hello.message_id = "modified".to_owned();
        assert!(
            verify_teacher_hello(&hello, &authority.public_key(), "device-1", cert, 50_000,)
                .is_err()
        );
    }
}
