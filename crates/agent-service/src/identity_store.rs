//! Постоянное хранение device identity T1 через Windows DPAPI.

use std::path::Path;

use transport::DeviceIdentity;
use zeroize::Zeroize;

#[derive(Debug, thiserror::Error)]
pub enum IdentityStoreError {
    #[error("ошибка файла device identity: {0}")]
    Io(#[from] std::io::Error),
    #[error("ошибка защиты device identity: {0}")]
    Platform(#[from] windows_platform::PlatformError),
    #[error("ошибка device identity: {0}")]
    Identity(#[from] transport::IdentityError),
    #[error("device identity сохранена частично; автоматическая ротация запрещена")]
    PartialIdentity,
    #[error("enrollment-код должен содержать от 6 до 64 ASCII-букв или цифр")]
    InvalidEnrollmentCode,
    #[error("enrollment-состояние сохранено частично")]
    PartialEnrollment,
}

#[derive(Debug, Clone)]
pub struct EnrollmentMaterial {
    pub credential: Vec<u8>,
    pub issuer_public_key: [u8; 32],
}

/// Загружает существующую identity либо один раз создаёт новую. Частично
/// сохранённая пара считается ошибкой, чтобы не менять enrolled identity тихо.
pub fn load_or_create(
    device_id: &str,
    certificate_path: &Path,
    protected_key_path: &Path,
) -> Result<DeviceIdentity, IdentityStoreError> {
    match (certificate_path.exists(), protected_key_path.exists()) {
        (true, true) => load(certificate_path, protected_key_path),
        (false, false) => create(device_id, certificate_path, protected_key_path),
        _ => Err(IdentityStoreError::PartialIdentity),
    }
}

fn load(
    certificate_path: &Path,
    protected_key_path: &Path,
) -> Result<DeviceIdentity, IdentityStoreError> {
    let certificate = std::fs::read(certificate_path)?;
    let protected_key = std::fs::read(protected_key_path)?;
    let private_key = windows_platform::crypto::unprotect_machine_secret(&protected_key)?;
    DeviceIdentity::from_der(certificate, private_key).map_err(Into::into)
}

fn create(
    device_id: &str,
    certificate_path: &Path,
    protected_key_path: &Path,
) -> Result<DeviceIdentity, IdentityStoreError> {
    let identity = DeviceIdentity::generate(device_id)?;
    let mut protected_key =
        windows_platform::crypto::protect_machine_secret(identity.private_key_der())?;

    if let Some(parent) = certificate_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_new(certificate_path, identity.certificate_der())?;
    if let Err(err) = write_new(protected_key_path, &protected_key) {
        let _ = std::fs::remove_file(certificate_path);
        protected_key.zeroize();
        return Err(err);
    }
    protected_key.zeroize();
    Ok(identity)
}

fn write_new(path: &Path, contents: &[u8]) -> Result<(), IdentityStoreError> {
    use std::io::Write;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    Ok(())
}

pub fn save_pending_enrollment_code(code: &str) -> Result<(), IdentityStoreError> {
    let code = code.trim().to_uppercase();
    if !(6..=64).contains(&code.len()) || !code.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(IdentityStoreError::InvalidEnrollmentCode);
    }
    let path = agent_core::config::pending_enrollment_code_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, code)?;
    Ok(())
}

pub fn load_pending_enrollment_code() -> Result<Option<String>, IdentityStoreError> {
    match std::fs::read_to_string(agent_core::config::pending_enrollment_code_path()) {
        Ok(code) => Ok(Some(code.trim().to_owned())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

pub fn load_enrollment() -> Result<Option<EnrollmentMaterial>, IdentityStoreError> {
    let credential_path = agent_core::config::device_credential_path();
    let issuer_path = agent_core::config::teacher_issuer_key_path();
    match (credential_path.exists(), issuer_path.exists()) {
        (false, false) => Ok(None),
        (true, true) => {
            let issuer_public_key = std::fs::read(issuer_path)?
                .try_into()
                .map_err(|_| IdentityStoreError::PartialEnrollment)?;
            Ok(Some(EnrollmentMaterial {
                credential: std::fs::read(credential_path)?,
                issuer_public_key,
            }))
        }
        _ => Err(IdentityStoreError::PartialEnrollment),
    }
}

pub fn save_enrollment(material: &EnrollmentMaterial) -> Result<(), IdentityStoreError> {
    let credential_path = agent_core::config::device_credential_path();
    let issuer_path = agent_core::config::teacher_issuer_key_path();
    std::fs::write(&credential_path, &material.credential)?;
    if let Err(err) = std::fs::write(&issuer_path, material.issuer_public_key) {
        let _ = std::fs::remove_file(credential_path);
        return Err(err.into());
    }
    let _ = std::fs::remove_file(agent_core::config::pending_enrollment_code_path());
    Ok(())
}
