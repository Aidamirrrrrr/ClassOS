//! Периодическая проверка обновлений агента (spec T8 §8, ADR-0015).
//!
//! Служба **не устанавливает** обновление сама: заменить собственные
//! бинарники вживую она не может (§8.4). Её задача — узнать о новой версии,
//! проверить подпись и хеш до записи чего-либо в каталог установки и
//! запустить `classos-updater.exe` отдельным процессом.
//!
//! Cloud здесь не является доверенной стороной: подпись издателя проверяется
//! на устройстве, поэтому подменённый ответ Cloud приводит к отказу, а не к
//! установке чужого файла.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use updater::{Channel, UpdateDecision, UpdateManifest, evaluate_manifest, verify_payload};

/// Как часто устройство спрашивает Cloud о новой версии.
///
/// Обновление агента не срочная операция: час задержки ничего не меняет, а
/// частый опрос создаёт лишнюю нагрузку на Cloud от каждого устройства.
pub const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Пауза перед первой проверкой: сразу после загрузки машины идёт логин
/// пользователя и старт урока, и это худший момент для скачивания файла.
pub const UPDATE_CHECK_INITIAL_DELAY: Duration = Duration::from_secs(5 * 60);

/// Ограничение размера файла обновления. Существует, чтобы ошибка или
/// подменённый ответ не могли занять весь диск до проверки хеша.
const MAX_PAYLOAD_BYTES: u64 = 256 * 1024 * 1024;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, thiserror::Error)]
pub enum UpdateCheckError {
    #[error("Cloud недоступен: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Cloud ответил статусом {0}")]
    Status(u16),
    #[error("файл обновления больше допустимого размера")]
    PayloadTooLarge,
    #[error("обновление отклонено: {0}")]
    Rejected(#[from] updater::UpdateError),
    #[error("не удалось подготовить обновление: {0}")]
    Staging(String),
}

/// Ответ `GET /v1/updates/check`.
///
/// Манифест приходит ровно в том виде, в каком его разбирает
/// `updater::parse_manifest`: единый формат на обе стороны закреплён
/// кросс-языковым тестовым вектором (ADR-0015).
#[derive(Debug, Deserialize)]
struct CheckResponse {
    update: Option<UpdateManifest>,
}

/// Адрес проверки обновлений.
///
/// Текущая версия и канал передаются устройством: Cloud не хранит, что на
/// устройстве установлено сейчас, и не должен угадывать.
pub fn check_url(base_url: &str, channel: Channel, current_version: &str) -> String {
    format!(
        "{}/v1/updates/check?channel={}&current_version={}",
        base_url.trim_end_matches('/'),
        channel.as_str(),
        current_version,
    )
}

/// Куда раскладываются манифест и файл перед запуском updater.
pub fn staged_paths(staging_dir: &Path) -> (PathBuf, PathBuf) {
    (
        staging_dir.join("manifest.json"),
        staging_dir.join("payload.bin"),
    )
}

/// Спрашивает Cloud о доступном обновлении.
async fn fetch_manifest(
    client: &reqwest::Client,
    base_url: &str,
    channel: Channel,
    current_version: &str,
) -> Result<Option<UpdateManifest>, UpdateCheckError> {
    let response = client
        .get(check_url(base_url, channel, current_version))
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(UpdateCheckError::Status(response.status().as_u16()));
    }
    Ok(response.json::<CheckResponse>().await?.update)
}

/// Скачивает файл обновления.
///
/// Размер проверяется до чтения тела: заявленный `Content-Length` больше
/// лимита — повод отказаться, а не начать качать.
async fn download_payload(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<u8>, UpdateCheckError> {
    let response = client.get(url).timeout(DOWNLOAD_TIMEOUT).send().await?;
    if !response.status().is_success() {
        return Err(UpdateCheckError::Status(response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_PAYLOAD_BYTES)
    {
        return Err(UpdateCheckError::PayloadTooLarge);
    }
    let bytes = response.bytes().await?;
    if bytes.len() as u64 > MAX_PAYLOAD_BYTES {
        return Err(UpdateCheckError::PayloadTooLarge);
    }
    Ok(bytes.to_vec())
}

/// Записывает проверенные манифест и файл в staging-каталог.
fn stage(
    staging_dir: &Path,
    manifest: &UpdateManifest,
    payload: &[u8],
) -> Result<(PathBuf, PathBuf), UpdateCheckError> {
    std::fs::create_dir_all(staging_dir)
        .map_err(|error| UpdateCheckError::Staging(error.to_string()))?;
    let (manifest_path, payload_path) = staged_paths(staging_dir);
    let text = serde_json::to_string(manifest)
        .map_err(|error| UpdateCheckError::Staging(error.to_string()))?;
    std::fs::write(&manifest_path, text)
        .map_err(|error| UpdateCheckError::Staging(error.to_string()))?;
    std::fs::write(&payload_path, payload)
        .map_err(|error| UpdateCheckError::Staging(error.to_string()))?;
    Ok((manifest_path, payload_path))
}

/// Одна итерация проверки: узнать, проверить, подготовить.
///
/// Возвращает пути к подготовленному обновлению либо `None`, если обновлять
/// нечего. Проверка хеша выполняется **до** записи файла в staging: устройство
/// не должно хранить непроверенный исполняемый файл даже временно.
pub async fn prepare_update(
    client: &reqwest::Client,
    base_url: &str,
    channel: Channel,
    current_version: &str,
    staging_dir: &Path,
) -> Result<Option<(PathBuf, PathBuf)>, UpdateCheckError> {
    let Some(manifest) = fetch_manifest(client, base_url, channel, current_version).await? else {
        return Ok(None);
    };
    // Подпись проверяется до скачивания: качать сотню мегабайт по манифесту,
    // который заведомо будет отвергнут, незачем.
    let publisher = updater::publisher_key()?;
    if evaluate_manifest(&publisher, &manifest, current_version, channel)?
        == UpdateDecision::AlreadyCurrent
    {
        return Ok(None);
    }
    let payload = download_payload(client, &manifest.url).await?;
    verify_payload(&manifest, &payload)?;
    Ok(Some(stage(staging_dir, &manifest, &payload)?))
}

/// Периодическая проверка обновлений на всё время работы службы.
///
/// Ошибка проверки не является поводом что-либо ломать: недоступный Cloud
/// означает «сегодня не обновились», а урок продолжается (инвариант 5).
pub async fn run_update_checks(
    base_url: String,
    channel: Channel,
    current_version: &'static str,
    staging_dir: PathBuf,
    updater_binary: PathBuf,
    cancellation: CancellationToken,
) {
    if base_url.is_empty() {
        tracing::info!(
            event = "UPDATE_CHECK_DISABLED",
            reason = "cloud_not_configured"
        );
        return;
    }
    let client = match reqwest::Client::builder().build() {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(error = %error, event = "UPDATE_CHECK_UNAVAILABLE");
            return;
        }
    };

    tokio::select! {
        _ = cancellation.cancelled() => return,
        _ = tokio::time::sleep(UPDATE_CHECK_INITIAL_DELAY) => {}
    }

    let mut tick = tokio::time::interval(UPDATE_CHECK_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => return,
            _ = tick.tick() => {}
        }

        match prepare_update(&client, &base_url, channel, current_version, &staging_dir).await {
            Ok(None) => tracing::debug!(event = "UPDATE_CHECK_NO_UPDATE"),
            Ok(Some((manifest_path, payload_path))) => {
                tracing::info!(event = "UPDATE_STAGED", channel = channel.as_str());
                // Дальше служба себя не обслуживает: заменять собственные
                // бинарники вживую нельзя, поэтому запускается отдельный
                // процесс, который остановит службу, поставит обновление и
                // откатится при провале health check (spec T8 §8.4).
                match std::process::Command::new(&updater_binary)
                    .arg(&manifest_path)
                    .arg(&payload_path)
                    .spawn()
                {
                    Ok(child) => {
                        tracing::info!(pid = child.id(), event = "UPDATER_SPAWNED");
                        return;
                    }
                    Err(error) => {
                        tracing::error!(error = %error, event = "UPDATER_SPAWN_FAILED");
                    }
                }
            }
            Err(error) => tracing::warn!(error = %error, event = "UPDATE_CHECK_FAILED"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_url_carries_channel_and_current_version() {
        assert_eq!(
            check_url("https://cloud.example.org/", Channel::Beta, "0.2.0"),
            "https://cloud.example.org/v1/updates/check?channel=beta&current_version=0.2.0",
        );
    }

    /// Кросс-языковой контракт с Cloud (ADR-0015).
    ///
    /// Тот же текст закреплён в `services/cloud/test/updates.test.ts`.
    /// Cloud, переименовавший поле, ломает этот тест, а не обновление на
    /// реальном устройстве.
    #[test]
    fn manifest_from_cloud_parses() {
        let body = format!(
            r#"{{"update":{{"version":"0.3.0","url":"https://updates.example.org/classos-0.3.0.bin","sha256":"{sha}","signature":"{sig}","minimum_supported_version":"0.1.0","release_channel":"stable"}}}}"#,
            sha = "0f".repeat(32),
            sig = "ab".repeat(64),
        );
        let response: CheckResponse = serde_json::from_str(&body).expect("манифест Cloud");
        let manifest = response.update.expect("обновление присутствует");
        assert_eq!(manifest.version, "0.3.0");
        assert_eq!(manifest.release_channel, "stable");
        assert_eq!(manifest.minimum_supported_version, "0.1.0");
        assert_eq!(manifest.signature, [0xab_u8; 64]);
    }

    /// Отсутствие обновления — штатный ответ, а не ошибка разбора.
    #[test]
    fn empty_update_is_not_an_error() {
        let response: CheckResponse =
            serde_json::from_str(r#"{"update":null}"#).expect("пустой ответ Cloud");
        assert!(response.update.is_none());
    }

    /// Манифест и файл лежат рядом в одном каталоге: updater получает оба
    /// пути аргументами и не ищет ничего сам.
    #[test]
    fn staged_paths_share_one_directory() {
        let (manifest, payload) = staged_paths(Path::new("/tmp/updates"));
        assert_eq!(manifest.parent(), payload.parent());
        assert_eq!(manifest.file_name().unwrap(), "manifest.json");
    }
}
