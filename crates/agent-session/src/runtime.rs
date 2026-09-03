//! Runtime Session Host: подключение, handshake, обработка Ping и
//! GetSessionInfo, завершение по Shutdown или разрыву связи. Только Windows.

use std::time::Duration;

use protocol::envelope::Payload;
use protocol::{Envelope, LOCAL_PROTOCOL_VERSION, Pong, SessionHello, SessionInfo};
use uuid::Uuid;

use crate::ipc_client::IpcClient;

/// После потери Service процесс ждёт короткий grace period и завершается.
/// Переподключения нет: Service запустит новый экземпляр.
const PARENT_DEATH_GRACE_PERIOD: Duration = Duration::from_secs(2);

fn new_message_id() -> String {
    Uuid::new_v4().to_string()
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
