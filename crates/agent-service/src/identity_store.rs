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
