//! Server-side Named Pipe IPC connection (spec §41-48, §58-65). Windows-only.
//!
//! Each Session Host launch gets its own uniquely-named pipe (spec §41-42),
//! so the natural shape here is "create one pipe, wait for exactly one
//! connection, then run the handshake/heartbeat protocol on it" rather than
//! a long-lived `accept()` loop serving many connections on one name.

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

/// A single, already-authenticated (at the OS level) server-side IPC
/// connection to one Session Host.
pub struct PipeConnection {
    reader: FramedReader<ReadHalf<NamedPipeServer>>,
    writer: FramedWriter<WriteHalf<NamedPipeServer>>,
    peer_pid: u32,
    peer_session_id: u32,
}

impl PipeConnection {
    /// Creates a new Named Pipe server instance named `pipe_name`, ACL'd to
    /// `user_sid` + SYSTEM only (spec §44-48), and waits for exactly one
    /// client to connect. On connection, independently resolves the
    /// client's PID and session id via Windows APIs rather than trusting
    /// anything the client will claim in `SessionHello` (spec §59-60,
    /// §132).
    pub async fn accept_one(pipe_name: &str, user_sid: &str) -> Result<Self> {
        // Scoped so `descriptor`/`security_attributes` (neither of which
        // is `Send`, being raw-pointer-backed) are fully dropped before the
        // `.connect().await` below, keeping this function's future `Send`
        // (required by `tokio::spawn` in runtime.rs).
        let server = {
            let descriptor =
                PipeSecurityDescriptor::for_session_user(user_sid).map_err(platform_err)?;
            let mut security_attributes = descriptor.as_security_attributes();

            // SAFETY: `security_attributes` is a valid, live
            // SECURITY_ATTRIBUTES referencing `descriptor`'s memory for the
            // duration of this call; Windows copies the security
            // descriptor into the kernel pipe object during
            // CreateNamedPipeW, so `descriptor` need not outlive the call
            // itself.
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
        // SAFETY: `raw_handle` is the just-connected pipe server's own
        // handle, valid for the duration of this call.
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
