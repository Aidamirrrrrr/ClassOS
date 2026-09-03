//! UDP multicast discovery для локальной сети T1.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use prost::Message;
use protocol::network::{ClassOsDeviceAnnouncement, PROTOCOL_VERSION};
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

/// Administratively scoped multicast-группа ClassOS.
pub const DISCOVERY_MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 67, 79);
/// UDP-порт публичных discovery-объявлений.
pub const DEFAULT_DISCOVERY_PORT: u16 = 45_900;
/// TCP-порт будущего защищённого control-канала T1.
pub const DEFAULT_CONTROL_PORT: u16 = 45_901;
const MAX_ANNOUNCEMENT_SIZE: usize = 16 * 1024;

/// Настройки периодической отправки discovery-объявлений.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryConfig {
    pub multicast_addr: Ipv4Addr,
    pub port: u16,
    pub interval: Duration,
    pub max_jitter: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            multicast_addr: DISCOVERY_MULTICAST_ADDR,
            port: DEFAULT_DISCOVERY_PORT,
            interval: Duration::from_secs(3),
            max_jitter: Duration::from_secs(2),
        }
    }
}

/// Принятое объявление вместе с фактическим сетевым адресом отправителя.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedAnnouncement {
    pub announcement: ClassOsDeviceAnnouncement,
    pub source: SocketAddr,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("ошибка UDP discovery: {0}")]
    Io(#[from] std::io::Error),
    #[error("повреждённое discovery-объявление: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("некорректное discovery-объявление: {reason}")]
    InvalidAnnouncement { reason: &'static str },
}

/// Проверяет только структуру недоверенного объявления, но не подтверждает
/// подлинность устройства.
pub fn validate_announcement(
    announcement: &ClassOsDeviceAnnouncement,
) -> Result<(), DiscoveryError> {
    if announcement.protocol_version == 0 {
        return Err(DiscoveryError::InvalidAnnouncement {
            reason: "версия протокола равна нулю",
        });
    }
    if announcement.device_id.trim().is_empty() {
        return Err(DiscoveryError::InvalidAnnouncement {
            reason: "отсутствует device_id",
        });
    }
    if announcement.hostname.trim().is_empty() {
        return Err(DiscoveryError::InvalidAnnouncement {
            reason: "отсутствует hostname",
        });
    }
    if announcement.control_port == 0 || announcement.control_port > u16::MAX.into() {
        return Err(DiscoveryError::InvalidAnnouncement {
            reason: "некорректный control_port",
        });
    }
    Ok(())
}

/// Кодирует и однократно отправляет объявление в multicast-группу.
pub async fn broadcast_once(
    socket: &UdpSocket,
    announcement: &ClassOsDeviceAnnouncement,
    config: DiscoveryConfig,
) -> Result<(), DiscoveryError> {
    validate_announcement(announcement)?;
    let payload = announcement.encode_to_vec();
    socket
        .send_to(
            &payload,
            SocketAddrV4::new(config.multicast_addr, config.port),
        )
        .await?;
    Ok(())
}

/// Периодически объявляет устройство до отмены задачи.
pub async fn announce_loop(
    announcement: ClassOsDeviceAnnouncement,
    config: DiscoveryConfig,
    cancellation: CancellationToken,
) -> Result<(), DiscoveryError> {
    validate_announcement(&announcement)?;
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    socket.set_multicast_ttl_v4(1)?;

    loop {
        broadcast_once(&socket, &announcement, config).await?;
        let jitter_limit = config.max_jitter.as_millis().min(u64::MAX.into()) as u64;
        let jitter = Duration::from_millis(fastrand::u64(0..=jitter_limit));
        tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            _ = tokio::time::sleep(config.interval + jitter) => {}
        }
    }
}

/// Открывает multicast listener и принимает одно корректное объявление.
pub async fn listen_once(config: DiscoveryConfig) -> Result<ReceivedAnnouncement, DiscoveryError> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, config.port)).await?;
    socket.join_multicast_v4(config.multicast_addr, Ipv4Addr::UNSPECIFIED)?;
    receive_one(&socket).await
}

async fn receive_one(socket: &UdpSocket) -> Result<ReceivedAnnouncement, DiscoveryError> {
    let mut buffer = [0_u8; MAX_ANNOUNCEMENT_SIZE];
    let (size, source) = socket.recv_from(&mut buffer).await?;
    let announcement = ClassOsDeviceAnnouncement::decode(&buffer[..size])?;
    validate_announcement(&announcement)?;
    Ok(ReceivedAnnouncement {
        announcement,
        source,
    })
}

/// Создаёт стандартное объявление текущей версии протокола.
pub fn new_announcement(
    device_id: String,
    hostname: String,
    room_hint: String,
    agent_version: String,
    ip: String,
    control_port: u16,
) -> ClassOsDeviceAnnouncement {
    ClassOsDeviceAnnouncement {
        protocol_version: PROTOCOL_VERSION,
        device_id,
        hostname,
        room_hint,
        agent_version,
        ip,
        control_port: control_port.into(),
    }
}

/// Определяет IPv4-адрес интерфейса с маршрутом по умолчанию, не отправляя
/// сетевых пакетов. Фактический адрес UDP-источника всё равно считается более
/// надёжным, поскольку само объявление недоверенное.
pub fn local_ipv4() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn announcement() -> ClassOsDeviceAnnouncement {
        new_announcement(
            "device-1".to_owned(),
            "PC-01".to_owned(),
            "room-a".to_owned(),
            "0.1.0".to_owned(),
            "192.0.2.10".to_owned(),
            DEFAULT_CONTROL_PORT,
        )
    }

    #[test]
    fn announcement_round_trip_preserves_public_fields() {
        let original = announcement();
        let decoded =
            ClassOsDeviceAnnouncement::decode(original.encode_to_vec().as_slice()).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn validation_rejects_missing_identity() {
        let mut invalid = announcement();
        invalid.device_id.clear();
        assert!(matches!(
            validate_announcement(&invalid),
            Err(DiscoveryError::InvalidAnnouncement { .. })
        ));
    }

    #[tokio::test]
    async fn unicast_socket_path_sends_and_receives_announcement() {
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let receiver_addr = receiver.local_addr().unwrap();
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let original = announcement();

        sender
            .send_to(&original.encode_to_vec(), receiver_addr)
            .await
            .unwrap();
        let received = receive_one(&receiver).await.unwrap();

        assert_eq!(received.announcement, original);
        assert_eq!(received.source.ip(), Ipv4Addr::LOCALHOST);
    }
}
