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

/// Maximum frame size the client will accept (50 MB).
/// Rejects absurdly large length fields from a malicious or buggy server
/// before allocating memory.
const MAX_FRAME_SIZE: usize = 50 * 1024 * 1024;

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

    /// Connect to server. If `insecure` is false, connection will fail
    /// because the server uses self-signed certs. Use `--insecure` flag
    /// until TOFU certificate pinning is implemented.
    pub async fn connect(&self, server_addr: SocketAddr, insecure: bool) -> Result<Connection> {
        // Install default crypto provider (ring)
        let _ = rustls::crypto::ring::default_provider().install_default();

        if !insecure {
            anyhow::bail!(
                "Server uses self-signed certificates. Use --insecure to connect \
                 (until TOFU cert pinning is implemented)."
            );
        }

        eprintln!("⚠️  WARNING: TLS certificate verification DISABLED (--insecure)");
        eprintln!("   Connection is encrypted but NOT authenticated — MITM attacks possible.\n");
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

    /// Receive stream metadata from the first unidirectional stream.
    /// Must be called once after connect(), before receive_frame().
    pub async fn receive_metadata(connection: &Connection) -> Result<mm_warp_common::StreamMetadata> {
        let mut stream = connection.accept_uni().await
            .context("Failed to accept metadata stream")?;
        let mut buf = [0u8; mm_warp_common::StreamMetadata::SIZE];
        stream.read_exact(&mut buf).await
            .context("Failed to read stream metadata")?;
        mm_warp_common::StreamMetadata::from_bytes(&buf)
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

        if len > MAX_FRAME_SIZE {
            anyhow::bail!(
                "Frame size {} exceeds maximum allowed size ({})",
                len, MAX_FRAME_SIZE,
            );
        }

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
            return Ok(Vec::new());
        }

        let packet = ffmpeg_next::Packet::copy(encoded_packet);

        self.decoder.send_packet(&packet)
            .context("Failed to send packet to decoder")?;

        let mut decoded = ffmpeg_next::frame::Video::empty();

        match self.decoder.receive_frame(&mut decoded) {
            Ok(_) => {
                // Validate decoded frame dimensions match our scaler
                let dw = decoded.width();
                let dh = decoded.height();
                if dw != self.width || dh != self.height {
                    // Recreate scaler for new dimensions
                    tracing::warn!("Decoded frame {}x{} differs from expected {}x{}, reinitializing scaler",
                        dw, dh, self.width, self.height);
                    self.width = dw;
                    self.height = dh;
                    self.scaler = ScaleContext::get(
                        ffmpeg_next::format::Pixel::YUV420P,
                        dw, dh,
                        ffmpeg_next::format::Pixel::RGBA,
                        dw, dh,
                        Flags::BILINEAR,
                    ).context("Failed to recreate scaler for new dimensions")?;
                }

                tracing::trace!("Decoded frame: {}x{}", dw, dh);

                // Create RGBA output frame
                let mut rgba_frame = ffmpeg_next::frame::Video::empty();
                rgba_frame.set_width(self.width);
                rgba_frame.set_height(self.height);
                rgba_frame.set_format(ffmpeg_next::format::Pixel::RGBA);
                let ret = unsafe {
                    ffmpeg_next::sys::av_frame_get_buffer(rgba_frame.as_mut_ptr(), 0)
                };
                if ret < 0 {
                    anyhow::bail!("av_frame_get_buffer failed for RGBA frame: error code {}", ret);
                }

                // Convert YUV420P → RGBA using cached swscale context
                self.scaler.run(&decoded, &mut rgba_frame)
                    .context("Failed to convert YUV420P to RGBA")?;

                // Copy RGBA data to output vector, respecting ffmpeg's linesize (stride).
                let stride = rgba_frame.stride(0);
                let row_bytes = (self.width * 4) as usize;
                let src = rgba_frame.data(0);
                let mut out = vec![0u8; (self.width * self.height * 4) as usize];
                for y in 0..self.height as usize {
                    let src_offset = y * stride;
                    let dst_offset = y * row_bytes;
                    out[dst_offset..dst_offset + row_bytes]
                        .copy_from_slice(&src[src_offset..src_offset + row_bytes]);
                }
                Ok(out)
            }
            Err(ffmpeg_next::Error::Other { errno: ffmpeg_next::util::error::EAGAIN }) => {
                Ok(Vec::new())
            }
            Err(e) => {
                Err(anyhow::anyhow!("Decoder error: {}", e))
            }
        }
    }
}

/// Skip certificate verification (INSECURE — only used with --insecure flag)
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
        assert!(result.is_ok());
    }

    #[test]
    fn test_h264_decoder() {
        let decoder_result = H264Decoder::new(1920, 1080);
        if let Ok(_decoder) = decoder_result {
            eprintln!("H.264 decoder created successfully");
        } else {
            eprintln!("H.264 decoder not available, skipping test");
        }
    }

    #[test]
    fn test_stream_metadata_round_trip() {
        let meta = mm_warp_common::StreamMetadata::new(3840, 2160, 60);
        let bytes = meta.to_bytes();
        let decoded = mm_warp_common::StreamMetadata::from_bytes(&bytes).unwrap();
        assert_eq!(meta, decoded);
    }
}
