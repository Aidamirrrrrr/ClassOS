//! `agent-core`: OS-independent business logic shared by `agent-service`
//! and `agent-session` — errors and config foundations here, with the
//! trait abstractions / session supervisor state machine added
//! separately.
//!
//! This crate must compile and its tests must pass on any host, including
//! non-Windows development machines. It has no dependency on
//! `windows-platform`.

pub mod config;
pub mod error;

pub use error::{AgentError, Result};
