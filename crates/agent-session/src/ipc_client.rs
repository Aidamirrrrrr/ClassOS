//! Клиентское IPC-соединение Named Pipe. Только Windows.

use std::time::Duration;

use protocol::Envelope;
use protocol::framing::{FramedReader, FramedWriter};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

const CONNECT_RETRY_INTERVAL: Duration = Duration::from_millis(200);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Клиентское соединение с каналом Service. Подлинность обеспечивается ACL
/// канала; отдельного секрета прикладного уровня нет.
pub struct IpcClient {
    reader: FramedReader<ReadHalf<NamedPipeClient>>,
    writer: FramedWriter<WriteHalf<NamedPipeClient>>,
}

impl IpcClient {
    /// Подключается к `pipe_name` с повторами до `CONNECT_TIMEOUT`, чтобы
    /// корректно пережить гонку запуска Service и Session Host.
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
