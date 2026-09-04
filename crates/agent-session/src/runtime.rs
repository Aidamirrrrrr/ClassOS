//! Runtime Session Host: подключение, handshake, обработка Ping и
//! GetSessionInfo, завершение по Shutdown или разрыву связи. Только Windows.

use std::time::Duration;

use protocol::envelope::Payload;
use protocol::{CaptureError, Envelope, LOCAL_PROTOCOL_VERSION, Pong, SessionHello, SessionInfo};
use screen_capture::{
    DxgiDesktopCapture, FrameEncoder, JpegEncoder, ScreenCapture, scale_to_max_width,
};
use uuid::Uuid;

use crate::ipc_client::IpcClient;

/// После потери Service процесс ждёт короткий grace period и завершается.
/// Переподключения нет: Service запустит новый экземпляр.
const PARENT_DEATH_GRACE_PERIOD: Duration = Duration::from_secs(2);

fn new_message_id() -> String {
    Uuid::new_v4().to_string()
}

fn capture_frame(
    display_id: u32,
    max_width: u32,
    jpeg_quality: u32,
) -> Result<protocol::Frame, screen_capture::CaptureError> {
    let mut capture = DxgiDesktopCapture::new()?;
    capture.start(display_id)?;
    let raw = scale_to_max_width(capture.next_frame()?, max_width.clamp(0, 3_840))?;
    capture.stop();
    let quality = (jpeg_quality != 0)
        .then(|| u8::try_from(jpeg_quality).unwrap_or(100))
        .unwrap_or(80);
    let mut encoder = JpegEncoder::new(quality);
    let encoded = encoder.encode(raw)?;
    Ok(protocol::Frame {
        display_id: encoded.display_id,
        width: encoded.width,
        height: encoded.height,
        encoded_data: encoded.data,
        format: encoded.format.to_owned(),
    })
}

fn public_capture_error(error: &screen_capture::CaptureError) -> (&'static str, &'static str) {
    match error {
        screen_capture::CaptureError::DisplayNotFound(_) => {
            ("DISPLAY_NOT_FOUND", "указанный дисплей не найден")
        }
        screen_capture::CaptureError::NotStarted => {
            ("CAPTURE_NOT_STARTED", "захват экрана не запущен")
        }
        screen_capture::CaptureError::BackendUnavailable => {
            ("CAPTURE_UNAVAILABLE", "захват экрана недоступен")
        }
        _ => ("CAPTURE_FAILED", "не удалось получить снимок экрана"),
    }
}

pub async fn run(session_id: u32, pipe_name: &str) -> std::io::Result<()> {
    let mut client = IpcClient::connect(pipe_name).await?;

    let hello = Envelope {
        message_id: new_message_id(),
        payload: Some(Payload::SessionHello(SessionHello {
            protocol_version: LOCAL_PROTOCOL_VERSION,
            session_id,
            pid: std::process::id(),
        })),
    };
    client
        .send(&hello)
        .await
        .map_err(|err| std::io::Error::other(err.to_string()))?;

    match client.recv().await {
        Ok(Some(envelope)) if matches!(&envelope.payload, Some(Payload::ServiceHello(_))) => {
            tracing::info!(event = "IPC_HANDSHAKE_OK");
        }
        Ok(_) => {
            tracing::error!(event = "IPC_HANDSHAKE_FAILED", reason = "no ServiceHello");
            return Ok(());
        }
        Err(err) => {
            tracing::error!(event = "IPC_HANDSHAKE_FAILED", error = %err);
            return Ok(());
        }
    }

    loop {
        let recv_result = tokio::time::timeout(PARENT_DEATH_GRACE_PERIOD * 5, client.recv()).await;

        let envelope = match recv_result {
            Ok(Ok(Some(envelope))) => envelope,
            Ok(Ok(None)) => {
                tracing::info!("service connection closed, exiting");
                break;
            }
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "protocol error on recv, exiting");
                break;
            }
            Err(_timeout) => {
                // Несколько интервалов нет даже Ping: Service недоступен,
                // поэтому не оставляем осиротевший процесс.
                tracing::warn!("no traffic from service, exiting");
                break;
            }
        };

        match envelope.payload {
            Some(Payload::Ping(ping)) => {
                let pong = Envelope {
                    message_id: new_message_id(),
                    payload: Some(Payload::Pong(Pong {
                        sequence: ping.sequence,
                    })),
                };
                if client.send(&pong).await.is_err() {
                    break;
                }
            }
            Some(Payload::GetSessionInfo(_)) => {
                let info = Envelope {
                    message_id: new_message_id(),
                    payload: Some(Payload::SessionInfo(SessionInfo {
                        session_id,
                        pid: std::process::id(),
                        username: std::env::var("USERNAME").unwrap_or_default(),
                        // В T0 состояние lock отслеживает Service через SCM;
                        // Session Host сообщает только начальное значение.
                        is_locked: false,
                    })),
                };
                if client.send(&info).await.is_err() {
                    break;
                }
            }
            Some(Payload::CaptureRequest(request)) => {
                let response = match capture_frame(
                    request.display_id,
                    request.max_width,
                    request.jpeg_quality,
                ) {
                    Ok(frame) => Envelope {
                        message_id: new_message_id(),
                        payload: Some(Payload::Frame(frame)),
                    },
                    Err(error) => {
                        tracing::warn!(error = %error, display_id = request.display_id, event = "CAPTURE_FAILED");
                        let (code, message) = public_capture_error(&error);
                        Envelope {
                            message_id: new_message_id(),
                            payload: Some(Payload::CaptureError(CaptureError {
                                code: code.to_owned(),
                                message: message.to_owned(),
                            })),
                        }
                    }
                };
                if client.send(&response).await.is_err() {
                    break;
                }
            }
            Some(Payload::Shutdown(_)) => {
                tracing::info!("received Shutdown, exiting");
                break;
            }
            Some(_) | None => {
                // Неожиданное сообщение журналируем и продолжаем без panic.
                tracing::warn!("received unexpected message, ignoring");
            }
        }
    }

    Ok(())
}
