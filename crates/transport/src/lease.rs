//! Signed classroom lease: офлайн-авторизация преподавателя.
//!
//! Решает задачу инварианта 5 (`CLAUDE.md`): пропал интернет — урок
//! продолжается. Cloud заранее выдаёт Teacher Console подписанный lease, а
//! Agent проверяет его **локально**, без сетевого запроса (spec T8 §7).
//!
//! Наличие lease само по себе ничего не значит: проверяется подпись, срок и
//! конкретное разрешение на конкретный кабинет (инвариант T8 §12.4).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

const LEASE_VERSION: u8 = 1;

/// Права, которые Cloud может выдать преподавателю.
///
/// Teacher не может выдать себе право сам: набор фиксируется в подписанном
/// lease, а расширить его можно только новым выпуском из Cloud (§12.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Permission {
    ViewClassroom,
    ControlClassroom,
    ApplyLessonProfile,
    RepairDevices,
}

impl Permission {
    /// Стабильное имя для подписи и журналов.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ViewClassroom => "view_classroom",
            Self::ControlClassroom => "control_classroom",
            Self::ApplyLessonProfile => "apply_lesson_profile",
            Self::RepairDevices => "repair_devices",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::ViewClassroom,
            Self::ControlClassroom,
            Self::ApplyLessonProfile,
            Self::RepairDevices,
        ]
        .into_iter()
        .find(|permission| permission.as_str() == value)
    }
}

/// Содержимое lease. Подписывается целиком.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassroomLease {
    pub teacher_id: String,
    pub organization_id: String,
    pub branch_id: String,
    pub allowed_rooms: Vec<String>,
    pub permissions: Vec<Permission>,
    pub issued_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
}

/// Lease вместе с подписью Cloud.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedLease {
    pub lease: ClassroomLease,
    pub signature: [u8; 64],
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LeaseError {
    #[error("некорректный публичный ключ issuer")]
    InvalidIssuerKey,
    #[error("подпись lease не прошла проверку")]
    InvalidSignature,
    #[error("срок действия lease истёк")]
    Expired,
    #[error("lease ещё не вступил в силу")]
    NotYetValid,
    #[error("кабинет не входит в lease")]
    RoomNotAllowed,
    #[error("действие не разрешено этим lease")]
    PermissionDenied,
}

/// Канонические байты для подписи.
///
/// Каждое поле пишется с префиксом длины: иначе lease на кабинет `"a"` со
/// вторым кабинетом `"b"` и lease на единственный кабинет `"ab"` дали бы
/// одинаковую строку для подписи.
fn payload(lease: &ClassroomLease) -> Vec<u8> {
    let mut bytes = vec![LEASE_VERSION];
    let push = |value: &[u8], bytes: &mut Vec<u8>| {
        bytes.extend_from_slice(&u32::try_from(value.len()).unwrap_or(u32::MAX).to_be_bytes());
        bytes.extend_from_slice(value);
    };
    push(lease.teacher_id.as_bytes(), &mut bytes);
    push(lease.organization_id.as_bytes(), &mut bytes);
    push(lease.branch_id.as_bytes(), &mut bytes);

    bytes.extend_from_slice(
        &u32::try_from(lease.allowed_rooms.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    for room in &lease.allowed_rooms {
        push(room.as_bytes(), &mut bytes);
    }

    bytes.extend_from_slice(
        &u32::try_from(lease.permissions.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    for permission in &lease.permissions {
        push(permission.as_str().as_bytes(), &mut bytes);
    }

    bytes.extend_from_slice(&lease.issued_at_unix_ms.to_be_bytes());
    bytes.extend_from_slice(&lease.expires_at_unix_ms.to_be_bytes());
    bytes
}

/// Выпускает lease. Выполняется в Cloud, а на устройстве — никогда.
pub fn issue(signing_key: &SigningKey, lease: ClassroomLease) -> SignedLease {
    let signature = signing_key.sign(&payload(&lease)).to_bytes();
    SignedLease { lease, signature }
}

/// Проверяет lease локально: подпись, срок, кабинет и право.
///
/// Все проверки обязательны и выполняются в этом порядке: сначала
/// криптография, потом бизнес-условия.
pub fn authorize(
    issuer_public_key: &[u8; 32],
    signed: &SignedLease,
    room_id: &str,
    permission: Permission,
    now_unix_ms: i64,
) -> Result<(), LeaseError> {
    let verifying_key =
        VerifyingKey::from_bytes(issuer_public_key).map_err(|_| LeaseError::InvalidIssuerKey)?;
    verifying_key
        .verify(
            &payload(&signed.lease),
            &Signature::from_bytes(&signed.signature),
        )
        .map_err(|_| LeaseError::InvalidSignature)?;

    if now_unix_ms < signed.lease.issued_at_unix_ms {
        return Err(LeaseError::NotYetValid);
    }
    if now_unix_ms >= signed.lease.expires_at_unix_ms {
        return Err(LeaseError::Expired);
    }
    if !signed
        .lease
        .allowed_rooms
        .iter()
        .any(|value| value == room_id)
    {
        return Err(LeaseError::RoomNotAllowed);
    }
    if !signed.lease.permissions.contains(&permission) {
        return Err(LeaseError::PermissionDenied);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7_u8; 32])
    }

    fn lease() -> ClassroomLease {
        ClassroomLease {
            teacher_id: "teacher-1".to_owned(),
            organization_id: "org-1".to_owned(),
            branch_id: "branch-1".to_owned(),
            allowed_rooms: vec!["room-2".to_owned(), "room-3".to_owned()],
            permissions: vec![Permission::ViewClassroom, Permission::ControlClassroom],
            issued_at_unix_ms: 1_000,
            expires_at_unix_ms: 100_000,
        }
    }

    #[test]
    fn valid_lease_authorizes_without_network() {
        let signing = key();
        let signed = issue(&signing, lease());
        assert_eq!(
            authorize(
                &signing.verifying_key().to_bytes(),
                &signed,
                "room-2",
                Permission::ControlClassroom,
                50_000
            ),
            Ok(())
        );
    }

    #[test]
    fn expired_lease_is_rejected() {
        let signing = key();
        let signed = issue(&signing, lease());
        assert_eq!(
            authorize(
                &signing.verifying_key().to_bytes(),
                &signed,
                "room-2",
                Permission::ViewClassroom,
                100_000
            ),
            Err(LeaseError::Expired)
        );
    }

    #[test]
    fn lease_from_the_future_is_rejected() {
        let signing = key();
        let signed = issue(&signing, lease());
        assert_eq!(
            authorize(
                &signing.verifying_key().to_bytes(),
                &signed,
                "room-2",
                Permission::ViewClassroom,
                500
            ),
            Err(LeaseError::NotYetValid)
        );
    }

    #[test]
    fn other_room_is_rejected() {
        let signing = key();
        let signed = issue(&signing, lease());
        assert_eq!(
            authorize(
                &signing.verifying_key().to_bytes(),
                &signed,
                "room-9",
                Permission::ViewClassroom,
                50_000
            ),
            Err(LeaseError::RoomNotAllowed)
        );
    }

    #[test]
    fn missing_permission_is_rejected() {
        let signing = key();
        let signed = issue(&signing, lease());
        // Teacher не может расширить свои права, просто попросив о них.
        assert_eq!(
            authorize(
                &signing.verifying_key().to_bytes(),
                &signed,
                "room-2",
                Permission::RepairDevices,
                50_000
            ),
            Err(LeaseError::PermissionDenied)
        );
    }

    #[test]
    fn tampering_with_any_field_breaks_the_signature() {
        let signing = key();
        let signed = issue(&signing, lease());
        let public = signing.verifying_key().to_bytes();

        for mutate in [
            (|value: &mut ClassroomLease| value.allowed_rooms.push("room-9".to_owned()))
                as fn(&mut ClassroomLease),
            |value: &mut ClassroomLease| value.permissions.push(Permission::RepairDevices),
            |value: &mut ClassroomLease| value.expires_at_unix_ms = i64::MAX,
            |value: &mut ClassroomLease| value.teacher_id = "teacher-2".to_owned(),
            |value: &mut ClassroomLease| value.branch_id = "branch-2".to_owned(),
        ] {
            let mut forged = signed.clone();
            mutate(&mut forged.lease);
            assert_eq!(
                authorize(
                    &public,
                    &forged,
                    "room-2",
                    Permission::ViewClassroom,
                    50_000
                ),
                Err(LeaseError::InvalidSignature)
            );
        }
    }

    #[test]
    fn lease_signed_by_another_issuer_is_rejected() {
        let signed = issue(&SigningKey::from_bytes(&[9_u8; 32]), lease());
        assert_eq!(
            authorize(
                &key().verifying_key().to_bytes(),
                &signed,
                "room-2",
                Permission::ViewClassroom,
                50_000
            ),
            Err(LeaseError::InvalidSignature)
        );
    }

    #[test]
    fn room_list_boundaries_are_unambiguous() {
        // Подпись lease на кабинеты ["a", "b"] не должна подходить lease на
        // единственный кабинет "ab".
        let signing = key();
        let mut two_rooms = lease();
        two_rooms.allowed_rooms = vec!["a".to_owned(), "b".to_owned()];
        let signed = issue(&signing, two_rooms);

        let mut forged = signed.clone();
        forged.lease.allowed_rooms = vec!["ab".to_owned()];
        assert_eq!(
            authorize(
                &signing.verifying_key().to_bytes(),
                &forged,
                "ab",
                Permission::ViewClassroom,
                50_000
            ),
            Err(LeaseError::InvalidSignature)
        );
    }

    /// Кросс-языковой вектор: тот же lease, тот же seed и та же подпись
    /// проверяются в `services/cloud/test/lease.test.ts`. Расхождение в
    /// кодировании между Cloud и агентом обязано ломать тесты здесь, а не
    /// проявляться отказом авторизации на реальном устройстве.
    #[test]
    fn cross_language_test_vector_matches_cloud() {
        let signing = key();
        let signed = issue(&signing, lease());
        let hex = signed
            .signature
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "85e6dcf80faa1280d7afd4867f1f1e146daef0a200a916d808e7ad901bdf8b1e\
7cc7f84d88bb27547f01be2866024d314261b22e7db555a9dfeb97caff076007"
        );
        let public = signing
            .verifying_key()
            .to_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            public,
            "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
        );
    }

    #[test]
    fn permission_names_round_trip() {
        for permission in [
            Permission::ViewClassroom,
            Permission::ControlClassroom,
            Permission::ApplyLessonProfile,
            Permission::RepairDevices,
        ] {
            assert_eq!(Permission::parse(permission.as_str()), Some(permission));
        }
        assert_eq!(Permission::parse("manage_billing"), None);
    }
}
