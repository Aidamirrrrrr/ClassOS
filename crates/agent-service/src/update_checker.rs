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
    publisher: &[u8; 32],
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
    if evaluate_manifest(publisher, &manifest, current_version, channel)?
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
    // Ключ читается один раз при старте: сборка без ключа издателя не должна
    // каждый час писать в журнал одну и ту же ошибку — она просто не
    // обновляется.
    let publisher = match updater::publisher_key() {
        Ok(key) => key,
        Err(error) => {
            tracing::warn!(error = %error, event = "UPDATE_CHECK_DISABLED");
            return;
        }
    };
    // Провайдер rustls устанавливается и здесь: reqwest паникует при создании
    // клиента без него, и полагаться на то, что кто-то сделал это раньше,
    // означало бы неявную зависимость от порядка инициализации.
    transport::install_crypto_provider();
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

        match prepare_update(
            &client,
            &publisher,
            &base_url,
            channel,
            current_version,
            &staging_dir,
        )
        .await
        {
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

    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Слушающий сокет и его адрес.
    ///
    /// Адрес нужен **до** формирования манифеста: манифест подписывает URL
    /// файла, поэтому порт должен быть известен заранее.
    async fn bind() -> (tokio::net::TcpListener, String) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        (listener, base_url)
    }

    /// Минимальный HTTP-сервер: префикс пути → тело ответа.
    ///
    /// Нужен, чтобы конвейер проверки обновлений действительно **исполнялся**
    /// в тестах, а не только компилировался. Иначе первый настоящий запуск
    /// этого пути произошёл бы на устройстве в школе.
    fn serve(
        listener: tokio::net::TcpListener,
        routes: Vec<(&'static str, Vec<u8>)>,
    ) -> tokio::task::JoinHandle<()> {
        let routes = Arc::new(routes);
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let routes = Arc::clone(&routes);
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 8192];
                    let read = stream.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();

                    let body = routes
                        .iter()
                        .find(|(route, _)| path.starts_with(route))
                        .map(|(_, body)| body.clone());
                    let response = match body {
                        Some(body) => {
                            let mut head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            )
                            .into_bytes();
                            head.extend_from_slice(&body);
                            head
                        }
                        None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec(),
                    };
                    let _ = stream.write_all(&response).await;
                    let _ = stream.shutdown().await;
                });
            }
        })
    }

    /// Манифест, подписанный ключом издателя, и сам публичный ключ.
    fn signed_manifest(payload: &[u8], version: &str, url: &str) -> (String, [u8; 32]) {
        use sha2::{Digest, Sha256};

        let signing = ed25519_dalek::SigningKey::from_bytes(&[5_u8; 32]);
        let mut manifest = UpdateManifest {
            version: version.to_owned(),
            url: url.to_owned(),
            sha256: Sha256::digest(payload)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            signature: [0_u8; 64],
            minimum_supported_version: "0.1.0".to_owned(),
            release_channel: Channel::Stable.as_str().to_owned(),
        };
        manifest.signature = updater::sign_manifest(&signing, &manifest);
        (
            serde_json::to_string(&manifest).unwrap(),
            signing.verifying_key().to_bytes(),
        )
    }

    /// Клиент для тестов. Провайдер обязателен: без него reqwest паникует —
    /// ровно та ошибка, которую этот набор тестов и обязан ловить.
    fn client() -> reqwest::Client {
        transport::install_crypto_provider();
        reqwest::Client::new()
    }

    fn staging_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("classos-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[tokio::test]
    async fn signed_update_is_downloaded_and_staged() {
        let payload = b"classos-release-payload".to_vec();
        let (listener, base_url) = bind().await;
        let (manifest_json, publisher) =
            signed_manifest(&payload, "0.3.0", &format!("{base_url}/payload.bin"));
        let server = serve(
            listener,
            vec![
                (
                    "/v1/updates/check",
                    format!(r#"{{"update":{manifest_json}}}"#).into_bytes(),
                ),
                ("/payload.bin", payload.clone()),
            ],
        );

        let staging = staging_dir("update");
        let staged = prepare_update(
            &client(),
            &publisher,
            &base_url,
            Channel::Stable,
            "0.2.0",
            &staging,
        )
        .await
        .expect("проверка обновления");

        let (manifest_path, payload_path) = staged.expect("обновление подготовлено");
        assert_eq!(std::fs::read(&payload_path).unwrap(), payload);
        assert!(
            std::fs::read_to_string(&manifest_path)
                .unwrap()
                .contains("0.3.0")
        );

        let _ = std::fs::remove_dir_all(&staging);
        server.abort();
    }

    /// Файл, не совпавший с манифестом, не должен оказаться на диске даже
    /// временно (инвариант T8 §12.3).
    #[tokio::test]
    async fn tampered_payload_is_not_staged() {
        let expected = b"classos-release-payload".to_vec();
        let (listener, base_url) = bind().await;
        let (manifest_json, publisher) =
            signed_manifest(&expected, "0.3.0", &format!("{base_url}/payload.bin"));
        let server = serve(
            listener,
            vec![
                (
                    "/v1/updates/check",
                    format!(r#"{{"update":{manifest_json}}}"#).into_bytes(),
                ),
                ("/payload.bin", b"substituted".to_vec()),
            ],
        );

        let staging = staging_dir("tampered");
        let result = prepare_update(
            &client(),
            &publisher,
            &base_url,
            Channel::Stable,
            "0.2.0",
            &staging,
        )
        .await;

        assert!(matches!(
            result,
            Err(UpdateCheckError::Rejected(
                updater::UpdateError::HashMismatch
            ))
        ));
        assert!(!staging.exists(), "staging-каталог не должен появиться");
        server.abort();
    }

    /// Манифест чужого издателя отвергается до скачивания файла: качать
    /// заведомо неприемлемое обновление незачем.
    #[tokio::test]
    async fn manifest_from_another_publisher_is_rejected_before_download() {
        let payload = b"classos-release-payload".to_vec();
        let (listener, base_url) = bind().await;
        let (manifest_json, _) =
            signed_manifest(&payload, "0.3.0", &format!("{base_url}/payload.bin"));
        // Файл намеренно не публикуется: если проверка подписи выполняется в
        // правильном порядке, до него дело не дойдёт.
        let server = serve(
            listener,
            vec![(
                "/v1/updates/check",
                format!(r#"{{"update":{manifest_json}}}"#).into_bytes(),
            )],
        );

        let staging = staging_dir("foreign");
        let result = prepare_update(
            &client(),
            &[7_u8; 32],
            &base_url,
            Channel::Stable,
            "0.2.0",
            &staging,
        )
        .await;

        assert!(matches!(
            result,
            Err(UpdateCheckError::Rejected(
                updater::UpdateError::InvalidSignature | updater::UpdateError::InvalidPublisherKey
            ))
        ));
        assert!(!staging.exists());
        server.abort();
    }

    /// Пустой ответ Cloud — штатная ситуация, а не ошибка.
    #[tokio::test]
    async fn no_update_leaves_disk_untouched() {
        let (listener, base_url) = bind().await;
        let server = serve(
            listener,
            vec![("/v1/updates/check", br#"{"update":null}"#.to_vec())],
        );

        let staging = staging_dir("empty");
        let staged = prepare_update(
            &client(),
            &[0_u8; 32],
            &base_url,
            Channel::Stable,
            "0.2.0",
            &staging,
        )
        .await
        .expect("пустой ответ");

        assert!(staged.is_none());
        assert!(!staging.exists());
        server.abort();
    }
}
