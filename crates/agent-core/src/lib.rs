//! `agent-core`: OS-independent business logic shared by `agent-service`
//! and `agent-session` — errors, config, domain types, trait abstractions
//! over Windows primitives, the session supervisor state machine, and
//! mocks for testing it without real Win32 (spec §142-146).
//!
//! This crate must compile and its tests must pass on any host, including
//! non-Windows development machines. It has no dependency on
//! `windows-platform`.

pub mod config;
pub mod domain;
pub mod error;
pub mod mocks;
pub mod supervisor;
pub mod traits;

pub use error::{AgentError, Result};
