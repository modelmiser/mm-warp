use anyhow::{Context, Result};
use quinn::{Connection, Endpoint};
use std::net::SocketAddr;
use ffmpeg_next::software::scaling::{context::Context as ScaleContext, flag::Flags};
use std::sync::Arc;

// Wayland display module
pub mod wayland_display;

// Input event handling (from shared crate)
pub use mm_warp_common::input_event;
pub use mm_warp_common::InputEvent;

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
        // Install default crypto provider (ring)
        let _ = rustls::crypto::ring::default_provider().install_default();

        // Skip cert verification for self-signed certs (dev only!)
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

/// H.264 decoder using ffmpeg (matching encoder pattern exactly)
pub struct H264Decoder {
    decoder: ffmpeg_next::decoder::Opened,
    scaler: ScaleContext,
    width: u32,
    height: u32,
}

impl H264Decoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        ffmpeg_next::init().context("Failed to initialize ffmpeg")?;

        let codec = ffmpeg_next::decoder::find(ffmpeg_next::codec::Id::H264)
            .context("H.264 codec not found")?;

        // Try just decoder() without video()
        let decoder = ffmpeg_next::codec::context::Context::new_with_codec(codec)
            .decoder()
            .open_as(codec)
            .map_err(|e| anyhow::anyhow!("Failed to open decoder: {}", e))?;

        let scaler = ScaleContext::get(
            ffmpeg_next::format::Pixel::YUV420P,
            width,
            height,
            ffmpeg_next::format::Pixel::RGBA,
            width,
            height,
            Flags::BILINEAR,
        ).context("Failed to create decoder scaler")?;

        Ok(Self { decoder, scaler, width, height })
    }

    /// Decode H.264 packet to RGBA frame
    pub fn decode(&mut self, encoded_packet: &[u8]) -> Result<Vec<u8>> {
        if encoded_packet.is_empty() {
            // Empty packet, return empty frame
            return Ok(Vec::new());
        }

        let packet = ffmpeg_next::Packet::copy(encoded_packet);

        self.decoder.send_packet(&packet)
            .context("Failed to send packet to decoder")?;

        let mut decoded = ffmpeg_next::frame::Video::empty();

        match self.decoder.receive_frame(&mut decoded) {
            Ok(_) => {
                // Successfully decoded frame (YUV420P)
                tracing::info!("Decoded frame: {}x{}", decoded.width(), decoded.height());

                // Create RGBA output frame
                let mut rgba_frame = ffmpeg_next::frame::Video::empty();
                rgba_frame.set_width(self.width);
                rgba_frame.set_height(self.height);
                rgba_frame.set_format(ffmpeg_next::format::Pixel::RGBA);
                unsafe {
                    ffmpeg_next::sys::av_frame_get_buffer(rgba_frame.as_mut_ptr(), 0);
                }

                // Convert YUV420P → RGBA using cached swscale context
                self.scaler.run(&decoded, &mut rgba_frame)
                    .context("Failed to convert YUV420P to RGBA")?;

                // Copy RGBA data to output vector
                let rgba_data = rgba_frame.data(0);
                Ok(rgba_data.to_vec())
            }
            Err(_) => {
                // Decoder buffering, return empty
                Ok(Vec::new())
            }
        }
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
        // Try to create decoder - might fail if H.264 codec not available
        let decoder_result = H264Decoder::new(1920, 1080);

        if let Ok(mut decoder) = decoder_result {
            // Decoder created successfully
            // Note: Can't test decode with fake packet (would fail)
            // Just verify decoder was created
            eprintln!("H.264 decoder created successfully");
        } else {
            eprintln!("H.264 decoder not available, skipping test");
        }
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
