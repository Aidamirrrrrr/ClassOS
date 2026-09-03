//! Client-side Named Pipe IPC connection (spec §41-43, §58, §126).
//! Windows-only.

use std::time::Duration;

use protocol::Envelope;
use protocol::framing::{FramedReader, FramedWriter};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

const CONNECT_RETRY_INTERVAL: Duration = Duration::from_millis(200);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// A client-side connection to the Service's Named Pipe. The pipe's ACL
/// (set up server-side, spec §44-48) is what authenticates this
/// connection; there is no separate application-level secret (spec §40).
pub struct IpcClient {
    reader: FramedReader<ReadHalf<NamedPipeClient>>,
    writer: FramedWriter<WriteHalf<NamedPipeClient>>,
}

impl IpcClient {
    /// Connects to `pipe_name`, retrying for up to `CONNECT_TIMEOUT` to
    /// absorb the inherent race between the Service creating its pipe
    /// server instance and this process starting up (both happen
    /// concurrently after launch; spec §168's "startup race" guidance
    /// applies symmetrically here).
    pub async fn connect(pipe_name: &str) -> std::io::Result<Self> {
        let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
        loop {
            match ClientOptions::new().open(pipe_name) {
                Ok(client) => {
                    let (read_half, write_half) = tokio::io::split(client);
                    return Ok(Self {
                        reader: FramedReader::new(read_half),
                        writer: FramedWriter::new(write_half),
                    });
                }
                Err(err) if tokio::time::Instant::now() < deadline => {
                    tracing::debug!(error = %err, "pipe not ready yet, retrying");
                    tokio::time::sleep(CONNECT_RETRY_INTERVAL).await;
                }
                Err(err) => return Err(err),
            }
        }
    }

    pub async fn send(&mut self, envelope: &Envelope) -> Result<(), protocol::ProtocolError> {
        self.writer.write_envelope(envelope).await
    }

    pub async fn recv(&mut self) -> Result<Option<Envelope>, protocol::ProtocolError> {
        self.reader.read_envelope().await
    }
}
