//! Подписанные enrollment credential и доказательство полномочий Teacher.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

const CREDENTIAL_VERSION: u8 = 1;
const SIGNATURE_SIZE: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum AuthorizationError {
    #[error("не удалось получить криптографическую случайность: {0}")]
    Random(#[from] getrandom::Error),
    #[error("некорректный публичный ключ issuer")]
    InvalidIssuerKey,
    #[error("повреждённый device credential")]
    InvalidCredential,
    #[error("подпись авторизации не прошла проверку")]
    InvalidSignature,
    #[error("срок действия device credential истёк")]
    ExpiredCredential,
    #[error("credential выпущен для другого устройства")]
    DeviceMismatch,
}

/// Локальный issuer Teacher Console из ADR-0007.
pub struct TeacherAuthority {
    signing_key: SigningKey,
}

impl TeacherAuthority {
    pub fn generate() -> Result<Self, AuthorizationError> {
        let mut secret = [0_u8; 32];
        getrandom::fill(&mut secret)?;
        Ok(Self {
            signing_key: SigningKey::from_bytes(&secret),
        })
    }

    pub fn from_secret(secret: &[u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(secret),
        }
    }

    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    pub fn issue_device_credential(
        &self,
        device_id: &str,
        certificate_der: &[u8],
        expires_at_unix_ms: i64,
    ) -> DeviceCredential {
        let fingerprint: [u8; 32] = Sha256::digest(certificate_der).into();
        let payload = credential_payload(device_id, fingerprint, expires_at_unix_ms);
        let signature = self.signing_key.sign(&payload).to_bytes();
        DeviceCredential {
            device_id: device_id.to_owned(),
            certificate_fingerprint: fingerprint,
            expires_at_unix_ms,
            signature,
        }
    }

    /// Подписывает конкретный envelope, исключая replay с другим message id.
    pub fn sign_teacher_hello(
        &self,
        teacher_session_id: &str,
        min_protocol: u32,
        max_protocol: u32,
        message_id: &str,
        timestamp_ms: i64,
    ) -> [u8; 64] {
        self.signing_key
            .sign(&teacher_proof_payload(
                teacher_session_id,
                min_protocol,
                max_protocol,
                message_id,
                timestamp_ms,
            ))
            .to_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCredential {
    pub device_id: String,
    pub certificate_fingerprint: [u8; 32],
    pub expires_at_unix_ms: i64,
    signature: [u8; 64],
}

impl DeviceCredential {
    pub fn encode(&self) -> Vec<u8> {
        let payload = credential_payload(
            &self.device_id,
            self.certificate_fingerprint,
            self.expires_at_unix_ms,
        );
        let mut encoded = payload;
        encoded.extend_from_slice(&self.signature);
        encoded
    }

    pub fn decode_and_verify(
        encoded: &[u8],
        issuer_public_key: &[u8; 32],
        expected_device_id: &str,
        expected_certificate_der: &[u8],
        now_unix_ms: i64,
    ) -> Result<Self, AuthorizationError> {
        if encoded.len() < 1 + 2 + 32 + 8 + SIGNATURE_SIZE {
            return Err(AuthorizationError::InvalidCredential);
        }
        let payload_len = encoded.len() - SIGNATURE_SIZE;
        let (payload, signature_bytes) = encoded.split_at(payload_len);
        let signature = Signature::from_slice(signature_bytes)
            .map_err(|_| AuthorizationError::InvalidCredential)?;
        let verifying_key = VerifyingKey::from_bytes(issuer_public_key)
            .map_err(|_| AuthorizationError::InvalidIssuerKey)?;
        verifying_key
            .verify(payload, &signature)
            .map_err(|_| AuthorizationError::InvalidSignature)?;

        let credential = decode_payload(payload, signature.to_bytes())?;
        if credential.device_id != expected_device_id
            || credential.certificate_fingerprint
                != <[u8; 32]>::from(Sha256::digest(expected_certificate_der))
        {
            return Err(AuthorizationError::DeviceMismatch);
        }
        if now_unix_ms >= credential.expires_at_unix_ms {
            return Err(AuthorizationError::ExpiredCredential);
        }
        Ok(credential)
    }

    pub fn verify_teacher_hello(
        issuer_public_key: &[u8; 32],
        teacher_session_id: &str,
        min_protocol: u32,
        max_protocol: u32,
        message_id: &str,
        timestamp_ms: i64,
        signature: &[u8],
    ) -> Result<(), AuthorizationError> {
        let verifying_key = VerifyingKey::from_bytes(issuer_public_key)
            .map_err(|_| AuthorizationError::InvalidIssuerKey)?;
        let signature =
            Signature::from_slice(signature).map_err(|_| AuthorizationError::InvalidSignature)?;
        verifying_key
            .verify(
                &teacher_proof_payload(
                    teacher_session_id,
                    min_protocol,
                    max_protocol,
                    message_id,
                    timestamp_ms,
                ),
                &signature,
            )
            .map_err(|_| AuthorizationError::InvalidSignature)
    }
}

fn credential_payload(device_id: &str, fingerprint: [u8; 32], expires_at: i64) -> Vec<u8> {
    let id = device_id.as_bytes();
    let id_len = u16::try_from(id.len()).unwrap_or(u16::MAX);
    let id = &id[..usize::from(id_len)];
    let mut payload = Vec::with_capacity(1 + 2 + id.len() + 32 + 8);
    payload.push(CREDENTIAL_VERSION);
    payload.extend_from_slice(&id_len.to_be_bytes());
    payload.extend_from_slice(id);
    payload.extend_from_slice(&fingerprint);
    payload.extend_from_slice(&expires_at.to_be_bytes());
    payload
}

fn decode_payload(
    payload: &[u8],
    signature: [u8; 64],
) -> Result<DeviceCredential, AuthorizationError> {
    if payload.first() != Some(&CREDENTIAL_VERSION) || payload.len() < 43 {
        return Err(AuthorizationError::InvalidCredential);
    }
    let id_len = usize::from(u16::from_be_bytes([payload[1], payload[2]]));
    if payload.len() != 1 + 2 + id_len + 32 + 8 {
        return Err(AuthorizationError::InvalidCredential);
    }
    let id_end = 3 + id_len;
    let device_id = std::str::from_utf8(&payload[3..id_end])
        .map_err(|_| AuthorizationError::InvalidCredential)?
        .to_owned();
    let fingerprint = payload[id_end..id_end + 32]
        .try_into()
        .map_err(|_| AuthorizationError::InvalidCredential)?;
    let expires_at_unix_ms = i64::from_be_bytes(
        payload[id_end + 32..id_end + 40]
            .try_into()
            .map_err(|_| AuthorizationError::InvalidCredential)?,
    );
    Ok(DeviceCredential {
        device_id,
        certificate_fingerprint: fingerprint,
        expires_at_unix_ms,
        signature,
    })
}

fn teacher_proof_payload(
    session_id: &str,
    min_protocol: u32,
    max_protocol: u32,
    message_id: &str,
    timestamp_ms: i64,
) -> Vec<u8> {
    format!("{session_id}\0{min_protocol}\0{max_protocol}\0{message_id}\0{timestamp_ms}")
        .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_is_bound_to_device_certificate() {
        let authority = TeacherAuthority::generate().unwrap();
        let credential = authority.issue_device_credential("device-1", b"certificate-a", 10_000);
        let encoded = credential.encode();
        assert!(
            DeviceCredential::decode_and_verify(
                &encoded,
                &authority.public_key(),
                "device-1",
                b"certificate-a",
                5_000,
            )
            .is_ok()
        );
        assert!(matches!(
            DeviceCredential::decode_and_verify(
                &encoded,
                &authority.public_key(),
                "device-1",
                b"certificate-b",
                5_000,
            ),
            Err(AuthorizationError::DeviceMismatch)
        ));
    }

    #[test]
    fn teacher_proof_rejects_changed_message() {
        let authority = TeacherAuthority::generate().unwrap();
        let signature = authority.sign_teacher_hello("teacher-1", 1, 1, "message-1", 42);
        assert!(
            DeviceCredential::verify_teacher_hello(
                &authority.public_key(),
                "teacher-1",
                1,
                1,
                "message-1",
                42,
                &signature,
            )
            .is_ok()
        );
        assert!(
            DeviceCredential::verify_teacher_hello(
                &authority.public_key(),
                "teacher-1",
                1,
                1,
                "message-2",
                42,
                &signature,
            )
            .is_err()
        );
    }
}
