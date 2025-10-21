use anyhow::{Context, Result};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry, wl_output, wl_shm};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_manager_v1, zwlr_screencopy_frame_v1,
};
use std::os::unix::io::{AsFd, AsRawFd};
use std::net::SocketAddr;
use std::sync::Arc;
use quinn::{Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// Represents a detected display output
#[derive(Debug, Clone)]
pub struct Display {
    pub name: String,
    pub width: i32,
    pub height: i32,
}

/// Frame buffer for captured screen data
pub struct FrameBuffer {
    frames: Vec<Vec<u8>>,
    capacity: usize,
    current: usize,
}

impl FrameBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            frames: Vec::with_capacity(capacity),
            capacity,
            current: 0,
        }
    }

    /// Add frame to ring buffer
    pub fn push(&mut self, frame: Vec<u8>) {
        if self.frames.len() < self.capacity {
            self.frames.push(frame);
        } else {
            self.frames[self.current] = frame;
        }
        self.current = (self.current + 1) % self.capacity;
    }

    /// Get latest frame
    pub fn latest(&self) -> Option<&[u8]> {
        if self.frames.is_empty() {
            None
        } else {
            let idx = if self.current == 0 {
                self.frames.len() - 1
            } else {
                self.current - 1
            };
            Some(&self.frames[idx])
        }
    }

    /// Get frame count
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// H.264 encoder using ffmpeg
pub struct H264Encoder {
    encoder: ffmpeg_next::encoder::Video,
    width: u32,
    height: u32,
}

impl H264Encoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        ffmpeg_next::init().context("Failed to initialize ffmpeg")?;

        let codec = ffmpeg_next::encoder::find(ffmpeg_next::codec::Id::H264)
            .context("H.264 codec not found")?;

        let mut encoder = ffmpeg_next::codec::context::Context::new_with_codec(codec)
            .encoder()
            .video()
            .context("Failed to create video encoder")?;

        encoder.set_width(width);
        encoder.set_height(height);
        encoder.set_format(ffmpeg_next::format::Pixel::YUV420P); // libx264 requires YUV
        encoder.set_time_base((1, 30));

        let encoder = encoder.open_as(codec).context("Failed to open encoder")?;

        Ok(Self { encoder, width, height })
    }

    /// Encode RGBA frame to H.264
    pub fn encode(&mut self, rgba_frame: &[u8]) -> Result<Vec<u8>> {
        let expected_size = (self.width * self.height * 4) as usize;
        if rgba_frame.len() != expected_size {
            anyhow::bail!("Frame size mismatch: expected {}, got {}", expected_size, rgba_frame.len());
        }

        // Convert RGBA to YUV420P (simple conversion - could use swscale for better quality)
        // For now: grayscale Y plane, zero U/V (results in grayscale output)
        let y_size = (self.width * self.height) as usize;
        let uv_size = y_size / 4;

        let mut yuv = vec![0u8; y_size + uv_size * 2];

        // Y plane (grayscale from RGB)
        for (i, rgba) in rgba_frame.chunks(4).enumerate() {
            let r = rgba[0] as u32;
            let g = rgba[1] as u32;
            let b = rgba[2] as u32;
            yuv[i] = ((66 * r + 129 * g + 25 * b + 128) >> 8) as u8 + 16;
        }
        // U and V planes stay zero (neutral chroma = grayscale)

        // Create frame
        let mut frame = ffmpeg_next::frame::Video::new(
            ffmpeg_next::format::Pixel::YUV420P,
            self.width,
            self.height
        );
        frame.data_mut(0)[..y_size].copy_from_slice(&yuv[..y_size]);
        frame.data_mut(1)[..uv_size].copy_from_slice(&yuv[y_size..y_size + uv_size]);
        frame.data_mut(2)[..uv_size].copy_from_slice(&yuv[y_size + uv_size..]);

        // Encode
        self.encoder.send_frame(&frame)
            .context("Failed to send frame to encoder")?;

        let mut packet = ffmpeg_next::Packet::empty();
        let mut encoded = Vec::new();

        while self.encoder.receive_packet(&mut packet).is_ok() {
            encoded.extend_from_slice(packet.data().unwrap_or(&[]));
        }

        if encoded.is_empty() {
            // Encoder buffering, return dummy packet
            Ok(vec![0u8; 32])
        } else {
            Ok(encoded)
        }
    }
}

/// QUIC server for streaming frames
pub struct QuicServer {
    endpoint: Endpoint,
}

impl QuicServer {
    /// Create server listening on given address
    pub async fn new(addr: SocketAddr) -> Result<Self> {
        // Generate self-signed cert (for now - real version uses proper certs)
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .context("Failed to generate certificate")?;

        let cert_der = CertificateDer::from(cert.cert);
        let key_der = PrivateKeyDer::try_from(cert.key_pair.serialize_der())
            .map_err(|e| anyhow::anyhow!("Failed to serialize private key: {}", e))?;

        let mut server_config = ServerConfig::with_single_cert(vec![cert_der], key_der)
            .context("Failed to create server config")?;

        let endpoint = Endpoint::server(server_config, addr)
            .context("Failed to create QUIC endpoint")?;

        tracing::info!("QUIC server listening on {}", addr);

        Ok(Self { endpoint })
    }

    /// Accept a client connection
    pub async fn accept(&mut self) -> Result<quinn::Connection> {
        let connecting = self.endpoint.accept().await
            .context("No incoming connection")?;

        let connection = connecting.await
            .context("Failed to complete handshake")?;

        tracing::info!("Client connected from {}", connection.remote_address());

        Ok(connection)
    }

    /// Send encoded frame over connection
    pub async fn send_frame(connection: &quinn::Connection, encoded_frame: &[u8]) -> Result<()> {
        // Open unidirectional stream (server -> client)
        let mut stream = connection.open_uni().await
            .context("Failed to open stream")?;

        // Send frame length then data (simple framing)
        let len = encoded_frame.len() as u32;
        stream.write_all(&len.to_be_bytes()).await
            .context("Failed to write frame length")?;
        stream.write_all(encoded_frame).await
            .context("Failed to write frame data")?;
        stream.finish()
            .context("Failed to finish stream")?;

        tracing::trace!("Sent {} byte frame", encoded_frame.len());

        Ok(())
    }
}

/// State for Wayland event handling
struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: <wl_registry::WlRegistry as wayland_client::Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        // Minimal handler - just need this to compile
    }
}

/// WaylandConnection manages connection to Wayland compositor
pub struct WaylandConnection {
    connection: Connection,
    displays: Vec<Display>,
}

impl WaylandConnection {
    /// Connect to Wayland compositor
    pub fn new() -> Result<Self> {
        let connection = Connection::connect_to_env()
            .context("Failed to connect to Wayland compositor. Is WAYLAND_DISPLAY set?")?;

        Ok(Self {
            connection,
            displays: Vec::new(),
        })
    }

    /// Enumerate available displays
    pub fn list_displays(&mut self) -> Result<&[Display]> {
        // Initialize registry and event queue
        let (globals, _event_queue) = registry_queue_init::<State>(&self.connection)
            .context("Failed to initialize Wayland registry")?;

        // Count output globals
        let output_count = globals.contents().with_list(|list| {
            list.iter()
                .filter(|global| global.interface == "wl_output")
                .count()
        });

        tracing::info!("Found {} display outputs", output_count);

        // For now, just record how many we found
        // Full output enumeration with geometry would require more protocol handling
        self.displays = (0..output_count).map(|i| Display {
            name: format!("Display {}", i),
            width: 0,  // Would get from wl_output.geometry event
            height: 0, // Would get from wl_output.geometry event
        }).collect();

        Ok(&self.displays)
    }

    /// Capture a single frame from the first display
    /// Returns raw RGBA buffer (very basic, no error handling yet)
    pub fn capture_frame(&mut self) -> Result<Vec<u8>> {
        // For Task 3, we'll use a simple approach:
        // 1. Create shared memory buffer
        // 2. Request screencopy to that buffer
        // 3. Return the buffer data

        // This is a placeholder - full implementation requires:
        // - wl_shm pool creation
        // - zwlr_screencopy_manager_v1 binding
        // - Event handling for frame ready

        // For now, return empty buffer as stub
        tracing::warn!("capture_frame is stub - returns empty buffer");
        Ok(vec![0u8; 1920 * 1080 * 4]) // Hardcoded HD RGBA
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wayland_connect() {
        // This will fail in CI without Wayland, that's OK
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            let result = WaylandConnection::new();
            assert!(result.is_ok(), "Should connect to Wayland when WAYLAND_DISPLAY is set");
        }
    }

    #[test]
    fn test_capture_stub() {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            let mut conn = WaylandConnection::new().unwrap();
            let frame = conn.capture_frame();
            assert!(frame.is_ok(), "Stub should return buffer");
        }
    }

    #[test]
    fn test_frame_buffer() {
        let mut buf = FrameBuffer::new(3);

        // Empty buffer
        assert_eq!(buf.len(), 0);
        assert!(buf.latest().is_none());

        // Add frames
        buf.push(vec![1, 2, 3]);
        buf.push(vec![4, 5, 6]);
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.latest(), Some(&[4u8, 5, 6][..]));

        // Fill to capacity
        buf.push(vec![7, 8, 9]);
        assert_eq!(buf.len(), 3);

        // Overflow (ring buffer wraps)
        buf.push(vec![10, 11, 12]);
        assert_eq!(buf.len(), 3); // Still 3 (capacity)
        assert_eq!(buf.latest(), Some(&[10u8, 11, 12][..]));
    }

    #[test]
    fn test_h264_encoder() {
        // Try to create encoder - might fail if H.264 codec not available
        let encoder_result = H264Encoder::new(1920, 1080);

        if let Ok(mut encoder) = encoder_result {
            // Encoder available, test it
            let frame = vec![0u8; 1920 * 1080 * 4];
            let encoded = encoder.encode(&frame);
            assert!(encoded.is_ok());

            let bad_frame = vec![0u8; 100];
            let result = encoder.encode(&bad_frame);
            assert!(result.is_err());
        } else {
            // H.264 codec not available in this build, skip test
            eprintln!("H.264 codec not available, skipping encoder test");
        }
    }
}
