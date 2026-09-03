//! `agent-service` library half: host-portable pieces (CLI parsing) live
//! here so they're testable without Windows. The privileged runtime
//! (service.rs, runtime.rs, windows_adapters.rs) is Windows-only and lives
//! directly in the `#[cfg(windows)]` binary (`src/main.rs` and its
//! submodules), per the split described in README-T0.md.

pub mod cli;
