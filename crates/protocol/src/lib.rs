//! Локальный IPC-протокол ClassOS: protobuf-типы, framing и версия handshake.

pub mod error;
pub mod framing;

pub use error::ProtocolError;

/// Версия локального IPC. При несовпадении handshake сразу завершается.
pub const LOCAL_PROTOCOL_VERSION: u32 = 1;

/// Максимальный допустимый размер frame.
pub const MAX_FRAME_SIZE: u32 = 64 * 1024;

include!(concat!(env!("OUT_DIR"), "/classos.local.v1.rs"));
