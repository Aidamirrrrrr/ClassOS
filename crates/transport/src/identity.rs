//! Криптографическая identity устройства и TLS server configuration.

use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

/// Сертификат и закрытый ключ устройства.
///
/// Закрытый ключ существует в памяти только для настройки TLS. Сохранять эту
/// структуру напрямую запрещено: platform-слой обязан защитить ключ средствами
/// ОС до записи на диск.
pub struct DeviceIdentity {
    certificate_der: Vec<u8>,
    private_key_der: Vec<u8>,
}

impl Drop for DeviceIdentity {
    fn drop(&mut self) {
        self.private_key_der.zeroize();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("не удалось создать identity устройства: {0}")]
    Generation(#[from] rcgen::Error),
    #[error("закрытый ключ устройства не подходит для TLS: {0}")]
    InvalidPrivateKey(#[from] rustls::Error),
    #[error("identity устройства содержит пустой сертификат или ключ")]
    Empty,
}

impl DeviceIdentity {
    /// Создаёт новую ECDSA identity с self-signed сертификатом, привязанным к
    /// постоянному `device_id` через Subject Alternative Name.
    pub fn generate(device_id: &str) -> Result<Self, IdentityError> {
        let dns_name = format!("{device_id}.device.classos.local");
        let CertifiedKey { cert, signing_key } = generate_simple_self_signed(vec![dns_name])?;
        Ok(Self {
            certificate_der: cert.der().to_vec(),
            private_key_der: signing_key.serialize_der(),
        })
    }

    /// Восстанавливает уже защищённую platform-слоем identity после чтения.
    pub fn from_der(
        certificate_der: Vec<u8>,
        private_key_der: Vec<u8>,
    ) -> Result<Self, IdentityError> {
        if certificate_der.is_empty() || private_key_der.is_empty() {
            return Err(IdentityError::Empty);
        }
        Ok(Self {
            certificate_der,
            private_key_der,
        })
    }

    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate_der
    }

    pub fn private_key_der(&self) -> &[u8] {
        &self.private_key_der
    }

    /// SHA-256 fingerprint служит стабильной привязкой enrollment к
    /// сертификату устройства, а не к изменяемому IP или hostname.
    pub fn certificate_fingerprint(&self) -> [u8; 32] {
        Sha256::digest(&self.certificate_der).into()
    }

    /// Создаёт rustls server configuration для control listener.
    pub fn server_config(&self) -> Result<ServerConfig, IdentityError> {
        let certificate = CertificateDer::from(self.certificate_der.clone());
        let private_key =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.private_key_der.clone()));
        Ok(ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate], private_key)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_identity_builds_tls_server_config() {
        let identity = DeviceIdentity::generate("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert!(!identity.certificate_der().is_empty());
        assert!(!identity.private_key_der().is_empty());
        identity.server_config().unwrap();
    }

    #[test]
    fn restored_identity_keeps_certificate_fingerprint() {
        let original = DeviceIdentity::generate("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let restored = DeviceIdentity::from_der(
            original.certificate_der().to_vec(),
            original.private_key_der().to_vec(),
        )
        .unwrap();
        assert_eq!(
            restored.certificate_fingerprint(),
            original.certificate_fingerprint()
        );
    }

    #[test]
    fn empty_identity_is_rejected() {
        assert!(matches!(
            DeviceIdentity::from_der(Vec::new(), Vec::new()),
            Err(IdentityError::Empty)
        ));
    }
}
