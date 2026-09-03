//! `windows-platform`: all raw, `unsafe` Win32 FFI for ClassOS T0 lives
//! here (spec §88-90). Business crates (`agent-core`, `agent-service`,
//! `agent-session`, `protocol`) must not contain raw unsafe Win32 code.
//!
//! This crate exposes only Windows primitives — no product domain
//! concepts (Teacher/Lesson/Student/AI/Organization, spec §148) — and can
//! only be type-checked against the `x86_64-pc-windows-msvc` target; it is
//! not buildable on non-Windows hosts and is not included in the default
//! host build (see `[target.'cfg(windows)'.dependencies]` in
//! `agent-service`/`agent-session`).

pub mod error;
pub mod handles;
pub mod pipes;
pub mod process;
pub mod security;
pub mod sessions;

pub use error::{PlatformError, Result};
