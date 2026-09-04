//! Runtime Session Host: подключение, handshake, обработка Ping и
//! GetSessionInfo, завершение по Shutdown или разрыву связи. Только Windows.

use std::time::Duration;

use protocol::envelope::Payload;
use protocol::{CaptureError, Envelope, LOCAL_PROTOCOL_VERSION, Pong, SessionHello, SessionInfo};
use remote_input::{
    MouseButton as InputMouseButton, RemoteInput, RemoteInputEvent as InputEvent, SendInputRemote,
    primary_display_size,
};
use screen_capture::{
    DxgiDesktopCapture, FrameEncoder, JpegEncoder, ScreenCapture, scale_to_max_width,
};
use uuid::Uuid;
use windows_platform::indicator::StudentIndicator;

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

fn to_input_event(event: protocol::RemoteInputEvent) -> Result<InputEvent, &'static str> {
    match event.event {
        Some(protocol::remote_input_event::Event::MouseMove(value)) => Ok(InputEvent::MouseMove {
            x: value.x,
            y: value.y,
        }),
        Some(protocol::remote_input_event::Event::MouseButton(value)) => {
            let button = match protocol::mouse_button::Button::try_from(value.button) {
                Ok(protocol::mouse_button::Button::Left) => InputMouseButton::Left,
                Ok(protocol::mouse_button::Button::Right) => InputMouseButton::Right,
                Ok(protocol::mouse_button::Button::Middle) => InputMouseButton::Middle,
                Err(_) => return Err("неизвестная кнопка мыши"),
            };
            Ok(InputEvent::MouseButton {
                button,
                is_down: value.is_down,
                x: value.x,
                y: value.y,
            })
        }
        Some(protocol::remote_input_event::Event::MouseWheel(value)) => {
            Ok(InputEvent::MouseWheel { delta: value.delta })
        }
        Some(protocol::remote_input_event::Event::KeyEvent(value)) => Ok(InputEvent::Key {
            virtual_key_code: value.virtual_key_code,
            is_down: value.is_down,
        }),
        None => Err("remote input не содержит события"),
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

    let mut remote_input = SendInputRemote::new();
    let mut remote_session: Option<String> = None;
    let indicator = StudentIndicator::default();
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
            Some(Payload::RemoteControlStart(request)) => {
                if let Err(error) = indicator.show() {
                    tracing::error!(error = %error, event = "REMOTE_CONTROL_INDICATOR_FAILED");
                    let response = Envelope {
                        message_id: new_message_id(),
                        payload: Some(Payload::CaptureError(CaptureError {
                            code: "INDICATOR_UNAVAILABLE".to_owned(),
                            message: "не удалось показать индикатор remote control".to_owned(),
                        })),
                    };
                    if client.send(&response).await.is_err() {
                        break;
                    }
                    continue;
                }
                remote_session = Some(request.session_id.clone());
                tracing::info!(session_id = %request.session_id, event = "REMOTE_CONTROL_INDICATOR_SHOWN");
                let response = Envelope {
                    message_id: new_message_id(),
                    payload: Some(Payload::RemoteControlStarted(
                        protocol::RemoteControlStarted {
                            session_id: request.session_id,
                        },
                    )),
                };
                if client.send(&response).await.is_err() {
                    break;
                }
            }
            Some(Payload::RemoteControlStop(request)) => {
                remote_session = None;
                indicator.hide();
                tracing::info!(reason = %request.reason, event = "REMOTE_CONTROL_INDICATOR_HIDDEN");
                let response = Envelope {
                    message_id: new_message_id(),
                    payload: Some(Payload::RemoteControlStopped(
                        protocol::RemoteControlStopped {
                            reason: request.reason,
                        },
                    )),
                };
                if client.send(&response).await.is_err() {
                    break;
                }
            }
            Some(Payload::RemoteInputEvent(event)) => {
                let Some(active_session) = remote_session.as_deref() else {
                    tracing::warn!(
                        event = "REMOTE_INPUT_REJECTED",
                        reason = "no_active_session"
                    );
                    continue;
                };
                let result = to_input_event(event).and_then(|input| {
                    let (width, height) =
                        primary_display_size().map_err(|_| "desktop недоступен")?;
                    remote_input
                        .apply(&input, width, height)
                        .map_err(|_| "SendInput отклонил событие")
                });
                match result {
                    Ok(()) => {
                        tracing::debug!(session_id = %active_session, event = "REMOTE_INPUT_APPLIED")
                    }
                    Err(reason) => {
                        tracing::warn!(session_id = %active_session, event = "REMOTE_INPUT_REJECTED", reason)
                    }
                }
            }
            Some(Payload::Shutdown(_)) => {
                if remote_session.take().is_some() {
                    indicator.hide();
                    tracing::info!(
                        event = "REMOTE_CONTROL_INDICATOR_HIDDEN",
                        reason = "service_shutdown"
                    );
                }
                tracing::info!("received Shutdown, exiting");
                break;
            }
            Some(_) | None => {
                // Неожиданное сообщение журналируем и продолжаем без panic.
                tracing::warn!("received unexpected message, ignoring");
            }
        }
    }

    if remote_session.take().is_some() {
        indicator.hide();
        tracing::info!(
            event = "REMOTE_CONTROL_INDICATOR_HIDDEN",
            reason = "service_connection_closed"
        );
    }

    Ok(())
}
