//! Протоколы ClassOS: локальный IPC, сетевые сообщения и общий framing.

pub mod error;
pub mod framing;

pub use error::ProtocolError;

/// Версия локального IPC. При несовпадении handshake сразу завершается.
pub const LOCAL_PROTOCOL_VERSION: u32 = 1;

/// Максимальный допустимый размер frame.
pub const MAX_FRAME_SIZE: u32 = 64 * 1024;

include!(concat!(env!("OUT_DIR"), "/classos.local.v1.rs"));

/// Сетевой протокол между Teacher Console и Student Agent.
pub mod network {
    /// Текущая версия сетевого протокола T1.
    pub const PROTOCOL_VERSION: u32 = 1;

    include!(concat!(env!("OUT_DIR"), "/classos.network.v1.rs"));

    /// Выбирает наибольшую общую версию протокола.
    pub fn negotiate_version(
        local_min: u32,
        local_max: u32,
        peer_min: u32,
        peer_max: u32,
    ) -> Option<u32> {
        let minimum = local_min.max(peer_min);
        let maximum = local_max.min(peer_max);
        (minimum <= maximum).then_some(maximum)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use prost::Message;

        #[test]
        fn negotiation_selects_highest_common_version() {
            assert_eq!(negotiate_version(1, 4, 2, 3), Some(3));
        }

        #[test]
        fn negotiation_accepts_single_version_overlap() {
            assert_eq!(negotiate_version(1, 2, 2, 5), Some(2));
        }

        #[test]
        fn negotiation_rejects_disjoint_ranges() {
            assert_eq!(negotiate_version(1, 2, 3, 4), None);
        }

        #[test]
        fn envelope_round_trip_preserves_device_hello() {
            let envelope = Envelope {
                protocol_version: PROTOCOL_VERSION,
                message_id: "message-1".to_owned(),
                timestamp_ms: 42,
                payload: Some(envelope::Payload::DeviceHello(DeviceHello {
                    device_id: "device-1".to_owned(),
                    hostname: "PC-01".to_owned(),
                    agent_version: "0.1.0".to_owned(),
                    os_version: "Windows 11".to_owned(),
                    capabilities: vec!["status".to_owned()],
                    min_protocol: 1,
                    max_protocol: 1,
                })),
            };

            let decoded = Envelope::decode(envelope.encode_to_vec().as_slice()).unwrap();
            assert_eq!(decoded, envelope);
        }
    }
}
