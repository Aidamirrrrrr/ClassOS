//! Собственный тип ошибок сохраняет направление зависимостей от service к
//! platform и не связывает низкоуровневый крейт с бизнес-логикой.

/// Ошибки Win32 с именем API и кодом, но без handles, tokens и секретов.
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
