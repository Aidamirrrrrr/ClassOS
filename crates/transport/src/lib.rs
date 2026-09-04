//! Независимый от продукта сетевой транспорт между Teacher Console и Agent.
//!
//! Discovery остаётся недоверенным каналом: этот крейт только доставляет
//! объявления и не принимает на их основании решений об авторизации.

/// Устанавливает процессный CryptoProvider rustls.
///
/// Вызывается один раз при старте до любой работы с TLS. Полагаться на
/// автоматический выбор нельзя: он работает, только пока во всём дереве
/// сборки включён ровно один провайдер, и любая новая зависимость с другим
/// провайдером ломает TLS в рантайме, а не на сборке. `ring` выбран потому,
/// что на нём уже построены `rustls` и `tokio-rustls` этого крейта.
///
/// Повторный вызов безвреден: провайдер устанавливается один раз.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

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
    HandshakeError, LeaseRequirement, TeacherAuthorization, VerifiedTeacher, build_device_hello,
    build_teacher_hello, verify_teacher_hello,
};
pub use identity::{DeviceIdentity, IdentityError};
pub use lease::{ClassroomLease, LeaseError, Permission, SignedLease, authorize, issue};
