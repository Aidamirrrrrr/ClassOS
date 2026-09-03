//! Серверное IPC-соединение Named Pipe (спека §41-48, §58-65), только Windows.
//!
//! Для каждого запуска Session Host создаётся канал с уникальным именем.
//! Он принимает ровно одно соединение и обслуживает его handshake/heartbeat.

use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;

use agent_core::error::{AgentError, Result};
use protocol::Envelope;
use protocol::framing::{FramedReader, FramedWriter};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions};
use windows::Win32::Foundation::HANDLE;
use windows_platform::security::PipeSecurityDescriptor;

fn platform_err(reason: impl std::fmt::Display) -> AgentError {
    AgentError::PipeCreateFailed {
        reason: reason.to_string(),
    }
}

/// Одно серверное IPC-соединение с Session Host, проверенное на уровне ОС.
pub struct PipeConnection {
    reader: FramedReader<ReadHalf<NamedPipeServer>>,
    writer: FramedWriter<WriteHalf<NamedPipeServer>>,
    peer_pid: u32,
    peer_session_id: u32,
}

impl PipeConnection {
    /// Создаёт Named Pipe `pipe_name` с доступом только для `user_sid` и
    /// SYSTEM и ожидает одного клиента. PID и session id проверяются через
    /// WinAPI независимо от данных `SessionHello`.
    pub async fn accept_one(pipe_name: &str, user_sid: &str) -> Result<Self> {
        // Ограничиваем время жизни объектов с raw-указателями до await,
        // чтобы future оставался `Send` для `tokio::spawn`.
        let server = {
            let descriptor =
                PipeSecurityDescriptor::for_session_user(user_sid).map_err(platform_err)?;
            let mut security_attributes = descriptor.as_security_attributes();

            // SAFETY: `security_attributes` действителен во время вызова и
            // ссылается на живой `descriptor`. Windows копирует descriptor
            // в объект ядра внутри CreateNamedPipeW.
            unsafe {
                ServerOptions::new()
                    .pipe_mode(PipeMode::Message)
                    .reject_remote_clients(true)
                    .max_instances(1)
                    .first_pipe_instance(true)
                    .create_with_security_attributes_raw(
                        pipe_name,
                        &mut security_attributes as *mut _ as *mut c_void,
                    )
            }
            .map_err(platform_err)?
        };

        server.connect().await.map_err(platform_err)?;

        let raw_handle = server.as_raw_handle();
        // SAFETY: `raw_handle` принадлежит подключённому серверу и
        // действителен на время вызова.
        let peer_pid = unsafe { windows_platform::pipes::client_process_id(HANDLE(raw_handle)) }
            .map_err(platform_err)?;
        let peer_session_id =
            windows_platform::sessions::session_id_for_process(peer_pid).map_err(platform_err)?;

        let (read_half, write_half) = tokio::io::split(server);
        Ok(Self {
            reader: FramedReader::new(read_half),
            writer: FramedWriter::new(write_half),
            peer_pid,
            peer_session_id,
        })
    }

    pub async fn send(&mut self, envelope: &Envelope) -> Result<()> {
        self.writer
            .write_envelope(envelope)
            .await
            .map_err(AgentError::from)
    }

    pub async fn recv(&mut self) -> Result<Option<Envelope>> {
        self.reader.read_envelope().await.map_err(AgentError::from)
    }

    pub fn peer_pid(&self) -> u32 {
        self.peer_pid
    }

    pub fn peer_session_id(&self) -> u32 {
        self.peer_session_id
    }
}
