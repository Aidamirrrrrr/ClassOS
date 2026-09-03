//! OS-independent domain types shared by the trait abstractions in
//! [`crate::traits`] and the [`crate::supervisor`] state machine. These use
//! only plain std types (no Win32) so `agent-core` compiles and is
//! unit-testable on any host (spec §142-146).

use std::ffi::OsString;
use std::path::PathBuf;

/// A discovered Windows session (spec §25-27). T0 only cares about the
/// physical console session, but the type itself carries no such
/// assumption — selection policy lives in the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub session_id: u32,
}

/// Specification for a process to launch inside a session (spec §34).
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
}

/// A process launched and tracked by the supervisor (spec §73).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedProcess {
    pub session_id: u32,
    pub pid: u32,
}
