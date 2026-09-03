//! Length-prefixed protobuf framing over any `AsyncRead`/`AsyncWrite`
//! transport (spec §49). Frame layout:
//!
//! ```text
//! 4-byte little-endian u32 length
//! + protobuf payload (length bytes)
//! ```
//!
//! The reader must not assume `one read == one message` (spec §99): partial
//! reads of both the length prefix and the payload must be handled
//! correctly. This module is transport-agnostic so it can run over a real
//! Windows Named Pipe in production and an in-memory duplex stream in unit
//! tests (spec §144-146).

use prost::Message;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{Envelope, MAX_FRAME_SIZE, error::ProtocolError};

const LENGTH_PREFIX_BYTES: usize = 4;

/// Reads length-prefixed protobuf `Envelope` frames from an async byte
/// stream.
pub struct FramedReader<R> {
    inner: R,
}

impl<R> FramedReader<R>
where
    R: AsyncRead + Unpin,
{
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    /// Reads a single frame and decodes it as an `Envelope`.
    ///
    /// Returns `Ok(None)` if the stream was closed cleanly before any bytes
    /// of a new frame were read (EOF between messages). A partial frame
    /// followed by EOF is reported as `ProtocolError::ConnectionClosed`.
    pub async fn read_envelope(&mut self) -> Result<Option<Envelope>, ProtocolError> {
        let mut len_buf = [0u8; LENGTH_PREFIX_BYTES];

        match self.inner.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(err) => return Err(ProtocolError::Io(err)),
        }

        let len = u32::from_le_bytes(len_buf);
        if len > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge {
                size: len,
                max: MAX_FRAME_SIZE,
            });
        }

        let mut payload = vec![0u8; len as usize];
        self.inner
            .read_exact(&mut payload)
            .await
            .map_err(|err| match err.kind() {
                std::io::ErrorKind::UnexpectedEof => ProtocolError::ConnectionClosed,
                _ => ProtocolError::Io(err),
            })?;

        let envelope = Envelope::decode(payload.as_slice())?;
        Ok(Some(envelope))
    }

    /// Consumes the reader, returning the inner transport.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

/// Writes length-prefixed protobuf `Envelope` frames to an async byte
/// stream.
pub struct FramedWriter<W> {
    inner: W,
}

impl<W> FramedWriter<W>
where
    W: AsyncWrite + Unpin,
{
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    /// Encodes and writes a single `Envelope` frame.
    pub async fn write_envelope(&mut self, envelope: &Envelope) -> Result<(), ProtocolError> {
        let payload = envelope.encode_to_vec();
        let len = u32::try_from(payload.len()).map_err(|_| ProtocolError::FrameTooLarge {
            size: u32::MAX,
            max: MAX_FRAME_SIZE,
        })?;

        if len > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge {
                size: len,
                max: MAX_FRAME_SIZE,
            });
        }

        self.inner.write_all(&len.to_le_bytes()).await?;
        self.inner.write_all(&payload).await?;
        self.inner.flush().await?;
        Ok(())
    }

    /// Consumes the writer, returning the inner transport.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ping;
    use crate::envelope::Payload;

    fn ping_envelope(sequence: u64) -> Envelope {
        Envelope {
            message_id: "test".to_string(),
            payload: Some(Payload::Ping(Ping {
                sequence,
                sent_at_unix_ms: 0,
            })),
        }
    }

    // Each DuplexStream endpoint already implements both AsyncRead and
    // AsyncWrite; there is no need to split() it (and doing so would keep
    // the underlying shared stream alive via the unused read/write half,
    // defeating EOF/close tests below).

    #[tokio::test]
    async fn round_trip_single_frame() {
        let (mut client, server) = tokio::io::duplex(4096);

        let mut writer = FramedWriter::new(&mut client);
        writer.write_envelope(&ping_envelope(42)).await.unwrap();

        let mut reader = FramedReader::new(server);
        let received = reader.read_envelope().await.unwrap().unwrap();

        match received.payload {
            Some(Payload::Ping(ping)) => assert_eq!(ping.sequence, 42),
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[tokio::test]
    async fn multiple_sequential_frames() {
        let (mut client, server) = tokio::io::duplex(8192);

        let mut writer = FramedWriter::new(&mut client);
        for seq in 0..5u64 {
            writer.write_envelope(&ping_envelope(seq)).await.unwrap();
        }

        let mut reader = FramedReader::new(server);
        for seq in 0..5u64 {
            let envelope = reader.read_envelope().await.unwrap().unwrap();
            match envelope.payload {
                Some(Payload::Ping(ping)) => assert_eq!(ping.sequence, seq),
                other => panic!("unexpected payload: {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn frame_too_large_is_rejected() {
        // Craft a raw length prefix that exceeds MAX_FRAME_SIZE and feed it
        // directly, bypassing FramedWriter (which also enforces the limit),
        // to verify the reader independently rejects oversized frames.
        let (mut client, server) = tokio::io::duplex(1024);

        let oversized_len = MAX_FRAME_SIZE + 1;
        client
            .write_all(&oversized_len.to_le_bytes())
            .await
            .unwrap();
        client.flush().await.unwrap();

        let mut reader = FramedReader::new(server);
        let err = reader.read_envelope().await.unwrap_err();
        assert!(matches!(err, ProtocolError::FrameTooLarge { .. }));
    }

    #[tokio::test]
    async fn partial_reads_are_reassembled() {
        let (mut client, server) = tokio::io::duplex(4096);

        let envelope = ping_envelope(7);
        let payload = envelope.encode_to_vec();
        let len = payload.len() as u32;
        let len_bytes = len.to_le_bytes();

        // Split the write into several small chunks straddling the length
        // prefix and payload boundary, to exercise read_exact's internal
        // looping rather than assuming one write == one read.
        client.write_all(&len_bytes[..2]).await.unwrap();
        client.flush().await.unwrap();
        client.write_all(&len_bytes[2..]).await.unwrap();
        client.flush().await.unwrap();

        let mid = payload.len() / 2;
        client.write_all(&payload[..mid]).await.unwrap();
        client.flush().await.unwrap();
        client.write_all(&payload[mid..]).await.unwrap();
        client.flush().await.unwrap();

        let mut reader = FramedReader::new(server);
        let received = reader.read_envelope().await.unwrap().unwrap();
        match received.payload {
            Some(Payload::Ping(ping)) => assert_eq!(ping.sequence, 7),
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[tokio::test]
    async fn clean_eof_between_messages_returns_none() {
        let (client, server) = tokio::io::duplex(1024);
        drop(client);

        let mut reader = FramedReader::new(server);
        assert!(reader.read_envelope().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn abrupt_disconnect_mid_frame_is_connection_closed() {
        let (mut client, server) = tokio::io::duplex(1024);

        // Announce a frame but never send its payload.
        client.write_all(&16u32.to_le_bytes()).await.unwrap();
        client.write_all(&[1, 2, 3]).await.unwrap();
        client.flush().await.unwrap();
        drop(client);

        let mut reader = FramedReader::new(server);
        let err = reader.read_envelope().await.unwrap_err();
        assert!(matches!(err, ProtocolError::ConnectionClosed));
    }

    #[tokio::test]
    async fn invalid_protobuf_payload_is_decode_error() {
        let (mut client, server) = tokio::io::duplex(1024);

        // A field tag with an invalid wire type combination for Envelope.
        let garbage: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, 0x0F];
        client
            .write_all(&(garbage.len() as u32).to_le_bytes())
            .await
            .unwrap();
        client.write_all(garbage).await.unwrap();
        client.flush().await.unwrap();

        let mut reader = FramedReader::new(server);
        let err = reader.read_envelope().await.unwrap_err();
        assert!(matches!(err, ProtocolError::Decode(_)));
    }
}
