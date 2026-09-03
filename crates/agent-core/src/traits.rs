//! Trait abstractions over Windows primitives (spec §142-146). Business
//! logic (the [`crate::supervisor::SessionSupervisor`] state machine) is
//! written against these traits only, so it can be unit-tested with the
//! mocks in [`crate::mocks`] without any real Win32 calls. The concrete
//! Win32-backed implementations live in `windows-platform` and are wired in
//! only by the `#[cfg(windows)]` real binaries.

use crate::domain::{ManagedProcess, ProcessSpec, Session};
use crate::error::Result;
use protocol::Envelope;

/// Discovers the active interactive session (spec §142).
pub trait SessionProvider: Send + Sync {
    fn active_console_session(&self) -> Result<Option<Session>>;
}

/// Launches and manages a process inside a given session (spec §143).
pub trait SessionProcessLauncher: Send + Sync {
    /// Launches `spec` inside `session_id`.
    fn launch(&self, session_id: u32, spec: &ProcessSpec) -> Result<ManagedProcess>;

    /// Returns whether the process identified by `pid` is still alive.
    fn is_alive(&self, pid: u32) -> bool;

    /// Terminates the managed process identified by `pid`. Must only ever
    /// be called on PIDs this launcher itself returned from `launch`
    /// (spec §72 — never target-by-name).
    fn terminate(&self, pid: u32) -> Result<()>;
}

/// A single accepted local IPC connection (spec §144).
#[async_trait::async_trait]
pub trait LocalIpcConnection: Send {
    async fn send(&mut self, envelope: &Envelope) -> Result<()>;

    /// Returns `Ok(None)` on clean connection close.
    async fn recv(&mut self) -> Result<Option<Envelope>>;

    /// The Windows session id this connection is bound to. Server-side
    /// implementations must independently verify this via
    /// `GetNamedPipeClientProcessId` / `ProcessIdToSessionId` rather than
    /// trusting anything the client claims in-band (spec §59-60, §132).
    fn peer_session_id(&self) -> Option<u32>;

    /// The client PID for this connection, as observed by the OS.
    fn peer_pid(&self) -> Option<u32>;
}

/// Accepts local IPC connections (spec §144).
#[async_trait::async_trait]
pub trait LocalIpcServer: Send + Sync {
    async fn accept(&self) -> Result<Box<dyn LocalIpcConnection>>;
}
