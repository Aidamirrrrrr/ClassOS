//! Ошибки framing и декодирования сообщений протокола.

/// Ошибки обработки локальных IPC-сообщений.
#[derive(thiserror::Error, Debug)]
pub enum ProtocolError {
    #[error("frame exceeds maximum size: {size} bytes (max {max})")]
    FrameTooLarge { size: u32, max: u32 },

    #[error("connection closed while reading frame")]
    ConnectionClosed,

    #[error("failed to decode protobuf payload: {0}")]
    Decode(#[from] prost::DecodeError),

    #[error("failed to encode protobuf payload: {0}")]
    Encode(#[from] prost::EncodeError),

    #[error("io error during framed read/write: {0}")]
    Io(#[from] std::io::Error),

    #[error("envelope did not contain a payload")]
    EmptyPayload,

    #[error("unexpected message type: expected {expected}, got {actual}")]
    UnexpectedMessage {
        expected: &'static str,
        actual: &'static str,
    },
}
