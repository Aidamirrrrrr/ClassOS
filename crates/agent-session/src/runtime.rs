//! Session Host runtime (spec §126-129): connect, handshake, then an event
//! loop that answers `Ping`/`GetSessionInfo` and exits on `Shutdown` or
//! disconnect. Windows-only.

use std::time::Duration;

use protocol::envelope::Payload;
use protocol::{Envelope, LOCAL_PROTOCOL_VERSION, Pong, SessionHello, SessionInfo};
use uuid::Uuid;

use crate::ipc_client::IpcClient;

/// If the Service connection is lost, don't hang around as an orphan
/// forever (spec §128) — exit after a short grace period in case it's a
/// transient blip, but this process does not attempt to reconnect
/// (spec §129): a fresh launch from the Service supersedes it.
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
                // No traffic at all (not even a Ping) for several
                // heartbeat intervals: the Service is gone. Don't wait
                // forever as an orphan (spec §128).
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
                        // T0's Session Host has no window/message loop
                        // observing WTS_SESSION_LOCK/UNLOCK yet (spec
                        // §76-77 is scoped to the Service's own session
                        // state tracking); always reporting false here is
                        // not a regression, just the T0 boundary.
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
                // Unknown/unexpected message: log and continue rather than
                // panic (spec §127).
                tracing::warn!("received unexpected message, ignoring");
            }
        }
    }

    Ok(())
}
