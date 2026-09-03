//! ClassOS local IPC protocol: generated protobuf types, framing, and the
//! shared protocol version constant used for the T0 Service <-> Session
//! Host handshake.

pub mod error;
pub mod framing;

pub use error::ProtocolError;

/// Local IPC protocol version. The handshake must fail fast on mismatch
/// (T0 spec §57).
pub const LOCAL_PROTOCOL_VERSION: u32 = 1;

/// Maximum accepted frame size, spec §49.
pub const MAX_FRAME_SIZE: u32 = 64 * 1024;

include!(concat!(env!("OUT_DIR"), "/classos.local.v1.rs"));
