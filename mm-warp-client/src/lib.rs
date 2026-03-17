use anyhow::{Context, Result};
use quinn::{Connection, Endpoint};
use std::net::SocketAddr;
use std::path::PathBuf; // used by TofuVerifier
use ffmpeg_next::software::scaling::{context::Context as ScaleContext, flag::Flags};
use std::sync::Arc;

// Wayland display module
pub mod wayland_display;

// Input event handling (from shared crate)
pub use mm_warp_common::input_event;
pub use mm_warp_common::InputEvent;

/// Maximum frame size the client will accept (5 MB).
/// A 4K H.264 keyframe is typically 50-500KB; 5MB is generous headroom.
const MAX_FRAME_SIZE: usize = 5 * 1024 * 1024;

use mm_warp_common::{config_dir, cert_fingerprint};

/// QUIC client for receiving frames
pub struct QuicClient {
    endpoint: Endpoint,
}

impl QuicClient {
    pub fn new() -> Result<Self> {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| anyhow::anyhow!("Failed to create client endpoint: {}", e))?;
        Ok(Self { endpoint })
    }

    /// Connect to server.
    /// - `insecure=true`: skip all cert verification (MITM possible)
    /// - `insecure=false`: use TOFU (trust on first use) cert pinning
    pub async fn connect(&self, server_addr: SocketAddr, insecure: bool) -> Result<Connection> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let crypto = if insecure {
            eprintln!("⚠️  WARNING: TLS certificate verification DISABLED (--insecure)");
            eprintln!("   Connection is encrypted but NOT authenticated — MITM attacks possible.\n");
            rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(SkipVerification))
                .with_no_client_auth()
        } else {
            // TOFU: trust on first use, verify on subsequent connections
            let known_hosts_path = config_dir().join("known_hosts");
            let verifier = TofuVerifier::new(known_hosts_path);
            rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(verifier))
                .with_no_client_auth()
        };

        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .context("Failed to create QUIC config")?
        ));

        // Use the server address as SNI so TOFU known_hosts keys are per-server
        let sni = server_addr.ip().to_string();
        let connection = self.endpoint.connect_with(client_config, server_addr, &sni)
            .context("Failed to initiate connection")?
            .await
            .context("Failed to complete handshake")?;

        tracing::info!("Connected to server at {}", server_addr);
        Ok(connection)
    }

    /// Receive stream metadata from the first unidirectional stream.
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
        let mut stream = connection.accept_uni().await
            .context("Failed to accept stream")?;

        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes).await
            .context("Failed to read frame length")?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        if len > MAX_FRAME_SIZE {
            anyhow::bail!("Frame size {} exceeds maximum ({})", len, MAX_FRAME_SIZE);
        }

        let mut frame = vec![0u8; len];
        stream.read_exact(&mut frame).await
            .context("Failed to read frame data")?;

        tracing::trace!("Received {} byte frame", len);
        Ok(frame)
    }
}

/// H.264 decoder using ffmpeg
pub struct H264Decoder {
    decoder: ffmpeg_next::decoder::Opened,
    scaler: ScaleContext,
    width: u32,
    height: u32,
}

impl H264Decoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 || width > 16384 || height > 16384 {
            anyhow::bail!("Invalid decoder dimensions {}x{} (max 16384)", width, height);
        }
        ffmpeg_next::init().context("Failed to initialize ffmpeg")?;

        let codec = ffmpeg_next::decoder::find(ffmpeg_next::codec::Id::H264)
            .context("H.264 codec not found")?;

        let decoder = ffmpeg_next::codec::context::Context::new_with_codec(codec)
            .decoder()
            .open_as(codec)
            .map_err(|e| anyhow::anyhow!("Failed to open decoder: {}", e))?;

        let scaler = ScaleContext::get(
            ffmpeg_next::format::Pixel::YUV420P, width, height,
            ffmpeg_next::format::Pixel::RGBA, width, height,
            Flags::BILINEAR,
        ).context("Failed to create decoder scaler")?;

        Ok(Self { decoder, scaler, width, height })
    }

    pub fn decode(&mut self, encoded_packet: &[u8]) -> Result<Vec<u8>> {
        if encoded_packet.is_empty() {
            return Ok(Vec::new());
        }

        let packet = ffmpeg_next::Packet::copy(encoded_packet);
        self.decoder.send_packet(&packet).context("Failed to send packet to decoder")?;

        let mut decoded = ffmpeg_next::frame::Video::empty();

        match self.decoder.receive_frame(&mut decoded) {
            Ok(_) => {
                let dw = decoded.width();
                let dh = decoded.height();
                if dw != self.width || dh != self.height {
                    tracing::warn!("Decoded frame {}x{} differs from expected {}x{}, reinitializing scaler",
                        dw, dh, self.width, self.height);
                    self.width = dw;
                    self.height = dh;
                    self.scaler = ScaleContext::get(
                        ffmpeg_next::format::Pixel::YUV420P, dw, dh,
                        ffmpeg_next::format::Pixel::RGBA, dw, dh,
                        Flags::BILINEAR,
                    ).context("Failed to recreate scaler")?;
                }

                tracing::trace!("Decoded frame: {}x{}", dw, dh);

                let mut rgba_frame = ffmpeg_next::frame::Video::empty();
                rgba_frame.set_width(self.width);
                rgba_frame.set_height(self.height);
                rgba_frame.set_format(ffmpeg_next::format::Pixel::RGBA);
                let ret = unsafe { ffmpeg_next::sys::av_frame_get_buffer(rgba_frame.as_mut_ptr(), 0) };
                if ret < 0 {
                    anyhow::bail!("av_frame_get_buffer failed: error code {}", ret);
                }

                self.scaler.run(&decoded, &mut rgba_frame).context("Failed to convert YUV420P to RGBA")?;

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
            Err(ffmpeg_next::Error::Other { errno: ffmpeg_next::util::error::EAGAIN }) => Ok(Vec::new()),
            Err(e) => Err(anyhow::anyhow!("Decoder error: {}", e)),
        }
    }
}

// ─── TLS Certificate Verifiers ───────────────────────────────────────────────

/// TOFU (Trust On First Use) certificate verifier — SSH-style cert pinning.
///
/// First connection to a server: save its certificate fingerprint to known_hosts.
/// Subsequent connections: verify the fingerprint matches. If it changed, refuse
/// the connection (possible MITM attack).
#[derive(Debug)]
struct TofuVerifier {
    known_hosts_path: PathBuf,
}

impl TofuVerifier {
    fn new(known_hosts_path: PathBuf) -> Self {
        Self { known_hosts_path }
    }

    fn lookup(&self, host: &str) -> Option<String> {
        let content = std::fs::read_to_string(&self.known_hosts_path).ok()?;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(2, ' ');
            if let (Some(h), Some(fp)) = (parts.next(), parts.next()) {
                if h == host {
                    return Some(fp.to_string());
                }
            }
        }
        None
    }

    fn save(&self, host: &str, fingerprint: &str) -> std::io::Result<()> {
        if let Some(parent) = self.known_hosts_path.parent() {
            std::fs::create_dir_all(parent)?;
            // Restrict config dir to owner-only access
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }
        // Don't duplicate: if host already exists, skip (user must manually remove old entry)
        if self.lookup(host).is_some() {
            return Ok(());
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.known_hosts_path)?;
        writeln!(file, "{} {}", host, fingerprint)?;
        // Restrict known_hosts to owner-only access
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.known_hosts_path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

impl rustls::client::danger::ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let fingerprint = cert_fingerprint(end_entity.as_ref());
        // Extract hostname/IP as a plain string for known_hosts key
        let host = match server_name.to_owned() {
            rustls::pki_types::ServerName::DnsName(name) => name.as_ref().to_string(),
            rustls::pki_types::ServerName::IpAddress(addr) => {
                // Convert pki_types IpAddr to std IpAddr for stable Display formatting
                use rustls::pki_types::IpAddr as PkiIp;
                match addr {
                    PkiIp::V4(v4) => std::net::Ipv4Addr::from(v4.as_ref().to_owned()).to_string(),
                    PkiIp::V6(v6) => std::net::Ipv6Addr::from(v6.as_ref().to_owned()).to_string(),
                }
            }
            _ => format!("{:?}", server_name),
        };

        match self.lookup(&host) {
            Some(known_fp) if known_fp == fingerprint => {
                // Known host, fingerprint matches
                eprintln!("✅ Server certificate verified (TOFU): SHA256:{}", &fingerprint[..16]);
                Ok(rustls::client::danger::ServerCertVerified::assertion())
            }
            Some(known_fp) => {
                // FINGERPRINT CHANGED — possible MITM
                eprintln!("🚨 SERVER CERTIFICATE HAS CHANGED!");
                eprintln!("   Expected: SHA256:{}", known_fp);
                eprintln!("   Got:      SHA256:{}", fingerprint);
                eprintln!("   This could indicate a man-in-the-middle attack.");
                eprintln!("   If the server was reinstalled, remove the old entry from:");
                eprintln!("   {}", self.known_hosts_path.display());
                Err(rustls::Error::General(
                    "Server certificate fingerprint changed (possible MITM). \
                     Remove old entry from known_hosts if server was reinstalled.".to_string()
                ))
            }
            None => {
                // New host — trust on first use
                eprintln!("🔑 New server, trusting certificate on first use:");
                eprintln!("   Fingerprint: SHA256:{}", fingerprint);
                if let Err(e) = self.save(&host, &fingerprint) {
                    eprintln!("   ⚠️  Failed to save to known_hosts: {}", e);
                } else {
                    eprintln!("   Saved to {}", self.known_hosts_path.display());
                }
                Ok(rustls::client::danger::ServerCertVerified::assertion())
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message, cert, dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message, cert, dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Skip all certificate verification (INSECURE — only used with --insecure flag)
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
        &self, _message: &[u8], _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self, _message: &[u8], _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
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

    #[test]
    fn test_cert_fingerprint() {
        let fp = cert_fingerprint(b"test cert data");
        assert_eq!(fp.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn test_tofu_verifier_lookup() {
        let dir = std::env::temp_dir().join("mm-warp-test-tofu");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("known_hosts");

        let verifier = TofuVerifier::new(path.clone());

        // No file yet — lookup returns None
        assert!(verifier.lookup("localhost").is_none());

        // Save and lookup
        verifier.save("localhost", "abc123").unwrap();
        assert_eq!(verifier.lookup("localhost"), Some("abc123".to_string()));
        assert!(verifier.lookup("other").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
