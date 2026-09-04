//! Независимый от продукта сетевой транспорт между Teacher Console и Agent.
//!
//! Discovery остаётся недоверенным каналом: этот крейт только доставляет
//! объявления и не принимает на их основании решений об авторизации.

pub mod authorization;
pub mod control;
pub mod discovery;
pub mod handshake;
pub mod identity;
pub mod lease;

pub use control::{
    ControlConnection, ControlError, ControlReader, ControlWriter, DeviceTransport,
    ServerControlConnection, TlsClient, TlsControlServer,
};

pub use authorization::{AuthorizationError, DeviceCredential, TeacherAuthority};
pub use discovery::{
    DEFAULT_CONTROL_PORT, DEFAULT_DISCOVERY_PORT, DISCOVERY_MULTICAST_ADDR, DiscoveryConfig,
    DiscoveryError, ReceivedAnnouncement, announce_loop, broadcast_once, listen_once, local_ipv4,
    new_announcement,
};
pub use handshake::{
    HandshakeError, VerifiedTeacher, build_device_hello, build_teacher_hello, verify_teacher_hello,
};
pub use identity::{DeviceIdentity, IdentityError};
pub use lease::{ClassroomLease, LeaseError, Permission, SignedLease, authorize, issue};
