//! Общий тип `AgentError` для ошибок компонентов агента.

/// Ошибка верхнего уровня. Ошибки WinAPI содержат имя API и числовой код,
/// но не раскрывают чувствительные данные.
#[derive(thiserror::Error, Debug)]
pub enum AgentError {
    #[error("windows API call failed: {api} (context: {context}, code: {code})")]
    WindowsApi {
        api: &'static str,
        context: String,
        code: i32,
    },

    #[error("no interactive session available")]
    SessionNotFound,

    #[error("failed to obtain user token for session {session_id}")]
    UserTokenFailed { session_id: u32 },

    #[error("failed to create user environment block")]
    EnvironmentCreationFailed,

    #[error("failed to launch process in session {session_id}: {reason}")]
    ProcessLaunchFailed { session_id: u32, reason: String },

    #[error("failed to create named pipe: {reason}")]
    PipeCreateFailed { reason: String },

    #[error("failed to construct pipe security descriptor: {reason}")]
    PipeSecurityFailed { reason: String },

    #[error("protocol error: {0}")]
    Protocol(#[from] protocol::ProtocolError),

    #[error("IPC handshake failed: {reason}")]
    HandshakeFailed { reason: String },

    #[error("heartbeat timed out after no pong for {elapsed_secs}s")]
    HeartbeatTimeout { elapsed_secs: u64 },

    #[error("shutdown error: {reason}")]
    Shutdown { reason: String },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config error: {reason}")]
    Config { reason: String },
}

pub type Result<T> = std::result::Result<T, AgentError>;
