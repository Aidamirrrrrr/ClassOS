//! Независимый от продукта сетевой транспорт между Teacher Console и Agent.
//!
//! Discovery остаётся недоверенным каналом: этот крейт только доставляет
//! объявления и не принимает на их основании решений об авторизации.

pub mod discovery;
pub mod identity;

pub use discovery::{
    DEFAULT_CONTROL_PORT, DEFAULT_DISCOVERY_PORT, DISCOVERY_MULTICAST_ADDR, DiscoveryConfig,
    DiscoveryError, ReceivedAnnouncement, announce_loop, broadcast_once, listen_once, local_ipv4,
    new_announcement,
};
pub use identity::{DeviceIdentity, IdentityError};
