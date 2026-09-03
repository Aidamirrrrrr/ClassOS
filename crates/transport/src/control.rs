//! TLS/TCP control transport и length-prefixed protobuf framing.

use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use prost::Message;
use protocol::network::Envelope;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error as TlsError, SignatureScheme};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::client::TlsStream as ClientTlsStream;
use tokio_rustls::server::TlsStream as ServerTlsStream;
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::DeviceIdentity;

const MAX_CONTROL_FRAME_SIZE: u32 = 256 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("ошибка control-соединения: {0}")]
    Io(#[from] std::io::Error),
    #[error("повреждённое protobuf-сообщение control-канала: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("control frame слишком большой: {actual} байт, максимум {maximum}")]
    FrameTooLarge { actual: u32, maximum: u32 },
    #[error("сервер не предоставил TLS-сертификат")]
    MissingPeerCertificate,
    #[error("некорректное TLS-имя устройства")]
    InvalidServerName,
}

/// Абстракция клиента, не зависящая от конкретного wire transport.
#[async_trait]
pub trait DeviceTransport {
    type Connection;

    async fn connect(&self, addr: SocketAddr) -> Result<Self::Connection, ControlError>;
}

/// Одно TLS-соединение с protobuf framing.
pub struct ControlConnection<S> {
    stream: S,
}

impl<S> ControlConnection<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn new(stream: S) -> Self {
        Self { stream }
    }

    pub async fn send(&mut self, envelope: &Envelope) -> Result<(), ControlError> {
        let payload = envelope.encode_to_vec();
        let size = u32::try_from(payload.len()).map_err(|_| ControlError::FrameTooLarge {
            actual: u32::MAX,
            maximum: MAX_CONTROL_FRAME_SIZE,
        })?;
        if size > MAX_CONTROL_FRAME_SIZE {
            return Err(ControlError::FrameTooLarge {
                actual: size,
                maximum: MAX_CONTROL_FRAME_SIZE,
            });
        }
        self.stream.write_all(&size.to_be_bytes()).await?;
        self.stream.write_all(&payload).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Возвращает `Ok(None)` при штатном EOF между сообщениями.
    pub async fn recv(&mut self) -> Result<Option<Envelope>, ControlError> {
        let mut size_bytes = [0_u8; 4];
        match self.stream.read_exact(&mut size_bytes).await {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(err) => return Err(err.into()),
        }
        let size = u32::from_be_bytes(size_bytes);
        if size > MAX_CONTROL_FRAME_SIZE {
            return Err(ControlError::FrameTooLarge {
                actual: size,
                maximum: MAX_CONTROL_FRAME_SIZE,
            });
        }
        let mut payload = vec![0_u8; size as usize];
        self.stream.read_exact(&mut payload).await?;
        Ok(Some(Envelope::decode(payload.as_slice())?))
    }
}

impl ControlConnection<ClientTlsStream<TcpStream>> {
    /// Возвращает сертификат peer для сохранения fingerprint после enrollment.
    pub fn peer_certificate_der(&self) -> Result<Vec<u8>, ControlError> {
        self.stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certificates| certificates.first())
            .map(|certificate| certificate.to_vec())
            .ok_or(ControlError::MissingPeerCertificate)
    }
}

/// TLS listener Student Agent.
pub struct TlsControlServer {
    listener: TcpListener,
    acceptor: TlsAcceptor,
}

impl TlsControlServer {
    pub async fn bind(addr: SocketAddr, identity: &DeviceIdentity) -> Result<Self, ControlError> {
        let listener = TcpListener::bind(addr).await?;
        let acceptor =
            TlsAcceptor::from(Arc::new(identity.server_config().map_err(|error| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, error)
            })?));
        Ok(Self { listener, acceptor })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, ControlError> {
        Ok(self.listener.local_addr()?)
    }

    pub async fn accept(
        &self,
    ) -> Result<(ControlConnection<ServerTlsStream<TcpStream>>, SocketAddr), ControlError> {
        let (stream, peer) = self.listener.accept().await?;
        let stream = self.acceptor.accept(stream).await?;
        Ok((ControlConnection::new(stream), peer))
    }
}

/// TLS-клиент Teacher Console. До enrollment допускает bootstrap-соединение,
/// после enrollment требует точного совпадения fingerprint сертификата.
pub struct TlsClient {
    connector: TlsConnector,
    server_name: ServerName<'static>,
}

impl TlsClient {
    pub fn bootstrap(device_id: &str) -> Result<Self, ControlError> {
        Self::new(device_id, None)
    }

    pub fn pinned(device_id: &str, fingerprint: [u8; 32]) -> Result<Self, ControlError> {
        Self::new(device_id, Some(fingerprint))
    }

    fn new(device_id: &str, fingerprint: Option<[u8; 32]>) -> Result<Self, ControlError> {
        let verifier = Arc::new(FingerprintVerifier { fingerprint });
        let config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();
        let name = format!("{device_id}.device.classos.local");
        let server_name =
            ServerName::try_from(name).map_err(|_| ControlError::InvalidServerName)?;
        Ok(Self {
            connector: TlsConnector::from(Arc::new(config)),
            server_name,
        })
    }
}

#[async_trait]
impl DeviceTransport for TlsClient {
    type Connection = ControlConnection<ClientTlsStream<TcpStream>>;

    async fn connect(&self, addr: SocketAddr) -> Result<Self::Connection, ControlError> {
        let stream = TcpStream::connect(addr).await?;
        let tls = self
            .connector
            .connect(self.server_name.clone(), stream)
            .await?;
        Ok(ControlConnection::new(tls))
    }
}

#[derive(Debug)]
struct FingerprintVerifier {
    fingerprint: Option<[u8; 32]>,
}

impl ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        if let Some(expected) = self.fingerprint
            && Sha256::digest(end_entity.as_ref()).as_slice() != expected
        {
            return Err(TlsError::General(
                "fingerprint сертификата устройства не совпадает".to_owned(),
            ));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use protocol::network::{Heartbeat, envelope};

    use super::*;

    fn heartbeat() -> Envelope {
        Envelope {
            protocol_version: 1,
            message_id: "heartbeat-1".to_owned(),
            timestamp_ms: 42,
            payload: Some(envelope::Payload::Heartbeat(Heartbeat {
                sequence: 7,
                sent_at_unix_ms: 42,
            })),
        }
    }

    #[tokio::test]
    async fn pinned_tls_connection_round_trips_envelope() {
        let device_id = "550e8400-e29b-41d4-a716-446655440000";
        let identity = DeviceIdentity::generate(device_id).unwrap();
        let server = TlsControlServer::bind("127.0.0.1:0".parse().unwrap(), &identity)
            .await
            .unwrap();
        let addr = server.local_addr().unwrap();
        let expected = heartbeat();
        let server_task = tokio::spawn(async move {
            let (mut connection, _) = server.accept().await.unwrap();
            let received = connection.recv().await.unwrap().unwrap();
            connection.send(&received).await.unwrap();
        });

        let client = TlsClient::pinned(device_id, identity.certificate_fingerprint()).unwrap();
        let mut connection = client.connect(addr).await.unwrap();
        connection.send(&expected).await.unwrap();
        assert_eq!(connection.recv().await.unwrap(), Some(expected));
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn wrong_certificate_fingerprint_is_rejected() {
        let device_id = "550e8400-e29b-41d4-a716-446655440000";
        let identity = DeviceIdentity::generate(device_id).unwrap();
        let server = TlsControlServer::bind("127.0.0.1:0".parse().unwrap(), &identity)
            .await
            .unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let _ = server.accept().await;
        });

        let client = TlsClient::pinned(device_id, [0_u8; 32]).unwrap();
        assert!(client.connect(addr).await.is_err());
        server_task.await.unwrap();
    }
}
