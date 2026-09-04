//! `classos-updater.exe` — применяет проверенное обновление агента.
//!
//! Запускается службой как отдельный процесс: заменить собственные бинарники
//! вживую служба не может (spec T8 §8.4). При провале health check выполняет
//! откат — это обязательное условие, а не опция (инвариант IV `CLAUDE.md`).

#[cfg(windows)]
fn main() {
    use std::path::PathBuf;
    use updater::{
        Channel, UpdateOutcome, evaluate_manifest, install_verified, verify_payload,
        windows_store::{ServiceHealthCheck, WindowsBinaryStore},
    };

    let mut args = std::env::args().skip(1);
    let manifest_path = args.next();
    let payload_path = args.next();
    let (Some(manifest_path), Some(payload_path)) = (manifest_path, payload_path) else {
        eprintln!("использование: classos-updater <manifest.json> <payload>");
        std::process::exit(2);
    };

    let manifest = match read_manifest(&manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("манифест недоступен: {error}");
            std::process::exit(1);
        }
    };
    let payload = match std::fs::read(&payload_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("файл обновления недоступен: {error}");
            std::process::exit(1);
        }
    };

    let publisher = match publisher_key() {
        Ok(key) => key,
        Err(error) => {
            eprintln!("публичный ключ издателя недоступен: {error}");
            std::process::exit(1);
        }
    };

    let current = env!("CARGO_PKG_VERSION");
    let channel = Channel::parse(&manifest.release_channel).unwrap_or(Channel::Stable);
    if let Err(error) = evaluate_manifest(&publisher, &manifest, current, channel) {
        eprintln!("обновление отклонено: {error}");
        std::process::exit(1);
    }
    if let Err(error) = verify_payload(&manifest, &payload) {
        eprintln!("обновление отклонено: {error}");
        std::process::exit(1);
    }

    let install_dir = PathBuf::from(r"C:\Program Files\ClassOS");
    let backup_dir = PathBuf::from(r"C:\ProgramData\ClassOS\state\update-backup");
    let store = WindowsBinaryStore::new(install_dir, backup_dir);

    match install_verified(&store, &ServiceHealthCheck::default(), &manifest, &payload) {
        Ok(UpdateOutcome::Installed { version }) => {
            println!("обновление до {version} установлено");
        }
        Ok(UpdateOutcome::RolledBack { version, reason }) => {
            eprintln!("обновление {version} откачено: {reason}");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("обновление не применено: {error}");
            std::process::exit(1);
        }
    }
}

/// Публичный ключ издателя вшит в бинарник: ключ, лежащий рядом на диске,
/// обесценил бы проверку подписи (spec T8 §12.3).
///
/// Ключ задаётся при сборке релиза через `CLASSOS_PUBLISHER_KEY_HEX`. Сборка
/// без ключа компилируется, но обновляться отказывается: тихо принимать
/// неподписанные обновления нельзя.
#[cfg(windows)]
fn publisher_key() -> Result<[u8; 32], String> {
    let Some(hex) = option_env!("CLASSOS_PUBLISHER_KEY_HEX") else {
        return Err("сборка выполнена без ключа издателя обновлений".to_owned());
    };
    let bytes: Result<Vec<u8>, _> = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16))
        .collect();
    let bytes = bytes.map_err(|error| error.to_string())?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| "ожидается 32 байта".to_owned())
}

#[cfg(windows)]
fn read_manifest(path: &str) -> Result<updater::UpdateManifest, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    updater::parse_manifest(&text).map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("classos-updater работает только на Windows");
}
