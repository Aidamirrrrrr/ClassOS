//! `agent-session` library half: host-portable pieces (CLI parsing) live
//! here so they're testable without Windows. The runtime (ipc_client.rs,
//! runtime.rs) is Windows-only and lives directly in the `#[cfg(windows)]`
//! binary, per the split described in README-T0.md.

pub mod cli;
