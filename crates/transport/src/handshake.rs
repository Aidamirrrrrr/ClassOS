//! Построение и проверка handshake Teacher ↔ Agent.

use protocol::network::envelope::Payload;
use protocol::network::{
    DeviceHello, Envelope, PROTOCOL_VERSION, TeacherHello, UpgradeRequired, negotiate_version,
};

use crate::lease::{LeaseError, Permission, SignedLease};
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
    #[error("classroom lease отклонён: {0}")]
    Lease(#[from] LeaseError),
    #[error("устройство требует classroom lease, но он не предъявлен")]
    MissingLease,
}

/// Чего устройство требует от преподавателя помимо credential (ADR-0016).
///
/// Режим определяется состоянием enrollment самого устройства, а не
/// содержимым запроса: иначе выбор режима достался бы вызывающей стороне.
#[derive(Debug, Clone, Copy)]
pub enum LeaseRequirement<'a> {
    /// Локальный enrollment (ADR-0007): lease не выдаётся и не проверяется.
    LocalEnrollment,
    /// Устройство зарегистрировано через Cloud: lease обязателен.
    Required {
        issuer_public_key: &'a [u8; 32],
        room_id: &'a str,
    },
}

/// Права преподавателя на этом устройстве.
///
/// Проверяются на каждой операции, а не один раз при подключении: срок
/// действия lease должен истекать посреди соединения, а не только на входе.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeacherAuthorization {
    /// Разграничения прав нет — граница ADR-0007, а не недоделка.
    LocalCredential,
    Lease {
        issuer_public_key: [u8; 32],
        room_id: String,
        lease: Box<SignedLease>,
    },
}

impl TeacherAuthorization {
    pub fn allows(&self, permission: Permission, now_unix_ms: i64) -> Result<(), LeaseError> {
        match self {
            Self::LocalCredential => Ok(()),
            Self::Lease {
                issuer_public_key,
                room_id,
                lease,
            } => {
                crate::lease::authorize(issuer_public_key, lease, room_id, permission, now_unix_ms)
            }
        }
    }

    /// Стабильное имя источника прав для журнала аудита.
    pub fn source(&self) -> &'static str {
        match self {
            Self::LocalCredential => "local_credential",
            Self::Lease { .. } => "classroom_lease",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedTeacher {
    pub teacher_session_id: String,
    pub negotiated_protocol: u32,
    pub authorization: TeacherAuthorization,
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
    lease: Option<&SignedLease>,
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
            classroom_lease: lease.map(SignedLease::encode).unwrap_or_default(),
        })),
    }
}

pub fn verify_teacher_hello(
    envelope: &Envelope,
    issuer_public_key: &[u8; 32],
    expected_device_id: &str,
    expected_certificate_der: &[u8],
    now_unix_ms: i64,
    lease_requirement: LeaseRequirement<'_>,
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

    let authorization = match lease_requirement {
        LeaseRequirement::LocalEnrollment => TeacherAuthorization::LocalCredential,
        LeaseRequirement::Required {
            issuer_public_key,
            room_id,
        } => {
            if hello.classroom_lease.is_empty() {
                return Err(HandshakeError::MissingLease);
            }
            let lease = SignedLease::decode(&hello.classroom_lease)?;
            // Право `ViewClassroom` есть у любой роли, которой вообще выдают
            // lease, поэтому проверка на входе отсеивает подпись, срок и чужой
            // кабинет, не решая за конкретную операцию.
            crate::lease::authorize(
                issuer_public_key,
                &lease,
                room_id,
                Permission::ViewClassroom,
                now_unix_ms,
            )?;
            TeacherAuthorization::Lease {
                issuer_public_key: *issuer_public_key,
                room_id: room_id.to_owned(),
                lease: Box::new(lease),
            }
        }
    };

    Ok(VerifiedTeacher {
        teacher_session_id: hello.teacher_session_id.clone(),
        negotiated_protocol: version,
        authorization,
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
            None,
        );
        let verified = verify_teacher_hello(
            &hello,
            &authority.public_key(),
            "device-1",
            cert,
            50_001,
            LeaseRequirement::LocalEnrollment,
        )
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
            None,
        );
        assert!(matches!(
            verify_teacher_hello(
                &hello,
                &authority.public_key(),
                "device-1",
                cert,
                90_001,
                LeaseRequirement::LocalEnrollment,
            ),
            Err(HandshakeError::StaleTimestamp)
        ));
        hello.message_id = "modified".to_owned();
        assert!(
            verify_teacher_hello(
                &hello,
                &authority.public_key(),
                "device-1",
                cert,
                50_000,
                LeaseRequirement::LocalEnrollment,
            )
            .is_err()
        );
    }

    fn lease_authority() -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[9_u8; 32])
    }

    fn lease_for(rooms: &[&str], permissions: Vec<Permission>) -> SignedLease {
        crate::lease::issue(
            &lease_authority(),
            crate::lease::ClassroomLease {
                teacher_id: "teacher-1".to_owned(),
                organization_id: "org-1".to_owned(),
                branch_id: "branch-1".to_owned(),
                allowed_rooms: rooms.iter().map(|room| (*room).to_owned()).collect(),
                permissions,
                issued_at_unix_ms: 40_000,
                expires_at_unix_ms: 100_000,
            },
        )
    }

    fn hello_with(
        lease: Option<&SignedLease>,
        authority: &TeacherAuthority,
        cert: &[u8],
    ) -> Envelope {
        let credential = authority.issue_device_credential("device-1", cert, 100_000);
        build_teacher_hello(
            authority,
            &credential,
            "teacher-1".to_owned(),
            "message-1".to_owned(),
            50_000,
            lease,
        )
    }

    /// Cloud-enrolled устройство не должно принимать преподавателя, который
    /// просто не приложил lease: иначе проверка прав обходится её пропуском.
    #[test]
    fn cloud_enrolled_device_refuses_connection_without_lease() {
        let authority = TeacherAuthority::generate().unwrap();
        let cert = b"device-certificate";
        let hello = hello_with(None, &authority, cert);
        let issuer = lease_authority().verifying_key().to_bytes();

        assert!(matches!(
            verify_teacher_hello(
                &hello,
                &authority.public_key(),
                "device-1",
                cert,
                50_001,
                LeaseRequirement::Required {
                    issuer_public_key: &issuer,
                    room_id: "room-1",
                },
            ),
            Err(HandshakeError::MissingLease)
        ));
    }

    /// Lease на соседний кабинет не даёт доступа к этому устройству.
    #[test]
    fn lease_for_another_room_is_rejected() {
        let authority = TeacherAuthority::generate().unwrap();
        let cert = b"device-certificate";
        let lease = lease_for(&["room-2"], vec![Permission::ViewClassroom]);
        let hello = hello_with(Some(&lease), &authority, cert);
        let issuer = lease_authority().verifying_key().to_bytes();

        assert!(matches!(
            verify_teacher_hello(
                &hello,
                &authority.public_key(),
                "device-1",
                cert,
                50_001,
                LeaseRequirement::Required {
                    issuer_public_key: &issuer,
                    room_id: "room-1",
                },
            ),
            Err(HandshakeError::Lease(LeaseError::RoomNotAllowed))
        ));
    }

    /// Права ограничены ровно тем, что перечислено в lease: подключиться с
    /// правом просмотра можно, взять управление — нет.
    #[test]
    fn granted_permissions_are_limited_to_the_lease() {
        let authority = TeacherAuthority::generate().unwrap();
        let cert = b"device-certificate";
        let lease = lease_for(&["room-1"], vec![Permission::ViewClassroom]);
        let hello = hello_with(Some(&lease), &authority, cert);
        let issuer = lease_authority().verifying_key().to_bytes();

        let verified = verify_teacher_hello(
            &hello,
            &authority.public_key(),
            "device-1",
            cert,
            50_001,
            LeaseRequirement::Required {
                issuer_public_key: &issuer,
                room_id: "room-1",
            },
        )
        .unwrap();

        assert_eq!(verified.authorization.source(), "classroom_lease");
        assert!(
            verified
                .authorization
                .allows(Permission::ViewClassroom, 50_001)
                .is_ok()
        );
        assert_eq!(
            verified
                .authorization
                .allows(Permission::ControlClassroom, 50_001),
            Err(LeaseError::PermissionDenied)
        );
    }

    /// Срок действия проверяется на каждой операции, а не только при
    /// подключении: lease, истёкший посреди урока, перестаёт действовать.
    #[test]
    fn expired_lease_stops_authorizing_mid_session() {
        let authority = TeacherAuthority::generate().unwrap();
        let cert = b"device-certificate";
        let lease = lease_for(
            &["room-1"],
            vec![Permission::ControlClassroom, Permission::ViewClassroom],
        );
        let hello = hello_with(Some(&lease), &authority, cert);
        let issuer = lease_authority().verifying_key().to_bytes();

        let verified = verify_teacher_hello(
            &hello,
            &authority.public_key(),
            "device-1",
            cert,
            50_001,
            LeaseRequirement::Required {
                issuer_public_key: &issuer,
                room_id: "room-1",
            },
        )
        .unwrap();

        assert!(
            verified
                .authorization
                .allows(Permission::ControlClassroom, 99_999)
                .is_ok()
        );
        assert_eq!(
            verified
                .authorization
                .allows(Permission::ControlClassroom, 100_001),
            Err(LeaseError::Expired)
        );
    }

    /// Локально зарегистрированное устройство работает как раньше — это
    /// зафиксированная граница ADR-0007, и она должна оставаться видимой.
    #[test]
    fn locally_enrolled_device_grants_everything_and_says_so() {
        let authority = TeacherAuthority::generate().unwrap();
        let cert = b"device-certificate";
        let hello = hello_with(None, &authority, cert);

        let verified = verify_teacher_hello(
            &hello,
            &authority.public_key(),
            "device-1",
            cert,
            50_001,
            LeaseRequirement::LocalEnrollment,
        )
        .unwrap();

        assert_eq!(verified.authorization.source(), "local_credential");
        assert!(
            verified
                .authorization
                .allows(Permission::RepairDevices, 50_001)
                .is_ok()
        );
    }
}
