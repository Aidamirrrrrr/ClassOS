//! `windows-platform` has its own error type rather than depending on
//! `agent-core::AgentError`, to keep the dependency graph as
//! `agent-service -> {agent-core, windows-platform}` (spec §147) with no
//! edge from `windows-platform` back into product/business crates
//! (spec §148 purity rule). Callers in `agent-service`/`agent-session`
//! convert `PlatformError` into `AgentError` at the boundary.

/// Errors from raw Win32 operations in this crate. Always carries enough
/// context (API name + numeric code where available) to be actionable in
/// logs (spec §86), and never leaks handles/tokens/secrets.
#[derive(thiserror::Error, Debug)]
pub enum PlatformError {
    #[error("{api} failed: {source}")]
    WindowsApi {
        api: &'static str,
        #[source]
        source: windows::core::Error,
    },

    #[error("no interactive console session")]
    NoActiveSession,

    #[error("{api} failed with unexpected result: {reason}")]
    Unexpected { api: &'static str, reason: String },
}

pub type Result<T> = std::result::Result<T, PlatformError>;
