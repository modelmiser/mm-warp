use anyhow::{Context, Result};
use quinn::{Connection, Endpoint};
use std::net::SocketAddr;
use std::sync::Arc;

/// QUIC client for receiving frames
pub struct QuicClient {
    endpoint: Endpoint,
}

impl QuicClient {
    /// Create client
    pub fn new() -> Result<Self> {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| anyhow::anyhow!("Failed to create client endpoint: {}", e))?;

        Ok(Self { endpoint })
    }

    /// Connect to server
    pub async fn connect(&self, server_addr: SocketAddr) -> Result<Connection> {
        // Skip cert verification for self-signed certs (dev only!)
        let mut root_store = rustls::RootCertStore::empty();
        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipVerification))
            .with_no_client_auth();

        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .context("Failed to create QUIC config")?
        ));

        let connection = self.endpoint.connect_with(client_config, server_addr, "localhost")
            .context("Failed to initiate connection")?
            .await
            .context("Failed to complete handshake")?;

        tracing::info!("Connected to server at {}", server_addr);

        Ok(connection)
    }

    /// Receive one frame from connection
    pub async fn receive_frame(connection: &Connection) -> Result<Vec<u8>> {
        // Accept incoming stream
        let mut stream = connection.accept_uni().await
            .context("Failed to accept stream")?;

        // Read frame length
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes).await
            .context("Failed to read frame length")?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        // Read frame data
        let mut frame = vec![0u8; len];
        stream.read_exact(&mut frame).await
            .context("Failed to read frame data")?;

        tracing::trace!("Received {} byte frame", len);

        Ok(frame)
    }
}

/// H.264 decoder (stub - real ffmpeg integration in extended version)
pub struct H264Decoder {
    width: u32,
    height: u32,
}

impl H264Decoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        Ok(Self { width, height })
    }

    /// Decode H.264 packet to RGBA frame
    /// Returns decoded frame (stub returns empty buffer)
    pub fn decode(&mut self, encoded_packet: &[u8]) -> Result<Vec<u8>> {
        // Stub: Real implementation would:
        // 1. Feed packet to ffmpeg decoder
        // 2. Get YUV420 frame
        // 3. Convert to RGBA
        // 4. Return buffer

        tracing::debug!("Decoded {} byte packet to {}x{} frame (stub)",
                       encoded_packet.len(), self.width, self.height);

        Ok(vec![0u8; (self.width * self.height * 4) as usize])
    }
}

/// Input event types
#[derive(Debug, Clone)]
pub enum InputEvent {
    KeyPress { key: u32 },
    KeyRelease { key: u32 },
    MouseMove { x: i32, y: i32 },
    MouseButton { button: u32, pressed: bool },
}

impl InputEvent {
    /// Serialize event to bytes for network transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            InputEvent::KeyPress { key } => {
                let mut buf = vec![1u8]; // Type: KeyPress
                buf.extend_from_slice(&key.to_be_bytes());
                buf
            }
            InputEvent::KeyRelease { key } => {
                let mut buf = vec![2u8]; // Type: KeyRelease
                buf.extend_from_slice(&key.to_be_bytes());
                buf
            }
            InputEvent::MouseMove { x, y } => {
                let mut buf = vec![3u8]; // Type: MouseMove
                buf.extend_from_slice(&x.to_be_bytes());
                buf.extend_from_slice(&y.to_be_bytes());
                buf
            }
            InputEvent::MouseButton { button, pressed } => {
                let mut buf = vec![4u8]; // Type: MouseButton
                buf.extend_from_slice(&button.to_be_bytes());
                buf.push(if *pressed { 1 } else { 0 });
                buf
            }
        }
    }

    /// Send input event over QUIC connection
    pub async fn send(connection: &Connection, event: InputEvent) -> Result<()> {
        let bytes = event.to_bytes();

        // Send as datagram (unreliable, fast)
        connection.send_datagram(bytes.into())
            .context("Failed to send input event")?;

        tracing::trace!("Sent input event: {:?}", event);

        Ok(())
    }
}

/// Skip certificate verification (dev only - accept self-signed certs)
#[derive(Debug)]
struct SkipVerification;

impl rustls::client::danger::ServerCertVerifier for SkipVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let result = QuicClient::new();
        if let Err(e) = &result {
            eprintln!("Client creation failed: {}", e);
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_h264_decoder() {
        let mut decoder = H264Decoder::new(1920, 1080).unwrap();

        // Decode stub packet
        let packet = vec![0u8; 1024];
        let decoded = decoder.decode(&packet);
        assert!(decoded.is_ok());

        let frame = decoded.unwrap();
        assert_eq!(frame.len(), 1920 * 1080 * 4);
    }

    #[test]
    fn test_input_event_serialization() {
        // Test each event type serializes
        let key_press = InputEvent::KeyPress { key: 42 };
        assert_eq!(key_press.to_bytes().len(), 5); // 1 type + 4 bytes key

        let mouse_move = InputEvent::MouseMove { x: 100, y: 200 };
        assert_eq!(mouse_move.to_bytes().len(), 9); // 1 type + 4 x + 4 y

        let mouse_btn = InputEvent::MouseButton { button: 1, pressed: true };
        assert_eq!(mouse_btn.to_bytes().len(), 6); // 1 type + 4 button + 1 pressed
    }
}
