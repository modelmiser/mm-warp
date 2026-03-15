use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::os::fd::AsFd;
use quinn::{Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use ffmpeg_next::software::scaling::{context::Context as ScaleContext, flag::Flags};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry, wl_output, wl_shm, wl_buffer, wl_shm_pool};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
    zwlr_screencopy_frame_v1::{ZwlrScreencopyFrameV1, Event as FrameEvent},
};

// Frame capture trait
pub mod capture;

// ext-image-copy-capture-v1 support (COSMIC, newer compositors)
pub mod ext_capture;

// Input event handling (from shared crate)
pub use mm_warp_common::input_event;
pub mod input_inject;
pub use mm_warp_common::InputEvent;
pub use input_inject::InputInjector;

/// Represents a detected display output
#[derive(Debug, Clone)]
pub struct Display {
    pub name: String,
    pub width: i32,
    pub height: i32,
}

/// H.264 encoder using ffmpeg
pub struct H264Encoder {
    encoder: ffmpeg_next::encoder::Video,
    scaler: ScaleContext,
    width: u32,
    height: u32,
    frame_count: i64,
    force_keyframe_next: bool,
}

// SAFETY: H264Encoder wraps ffmpeg contexts that use raw pointers internally.
// These are safe to move between threads as long as only one thread accesses
// them at a time. The pipelined server enforces this by transferring exclusive
// ownership to the encode thread, then back to main when the session ends.
unsafe impl Send for H264Encoder {}

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
        encoder.set_time_base((1, 60));
        encoder.set_frame_rate(Some((60, 1)));

        // Low latency settings for streaming
        encoder.set_gop(10); // Reasonable GOP (keyframe every ~0.3s at 30fps)
        encoder.set_max_b_frames(0); // No B-frames (reduces latency)

        // x264-specific options for zero-latency streaming
        let mut opts = ffmpeg_next::Dictionary::new();
        opts.set("preset", "ultrafast");      // Fast encoding
        opts.set("tune", "zerolatency");      // Optimize for low latency
        opts.set("intra-refresh", "1");       // Gradual intra refresh (smoother)
        opts.set("rc-lookahead", "0");        // No lookahead (immediate output)

        let encoder = encoder.open_with(opts).context("Failed to open encoder")?;

        let scaler = ScaleContext::get(
            ffmpeg_next::format::Pixel::RGBA,
            width,
            height,
            ffmpeg_next::format::Pixel::YUV420P,
            width,
            height,
            Flags::BILINEAR,
        ).context("Failed to create encoder scaler")?;

        Ok(Self { encoder, scaler, width, height, frame_count: 0, force_keyframe_next: false })
    }

    /// Force next frame to be a keyframe (IDR frame with SPS/PPS)
    /// Call this when a new client connects to ensure they get codec parameters
    pub fn force_keyframe(&mut self) {
        self.force_keyframe_next = true;
    }

    /// Encode RGBA frame to H.264
    pub fn encode(&mut self, rgba_frame: &[u8]) -> Result<Vec<u8>> {
        let expected_size = (self.width * self.height * 4) as usize;
        if rgba_frame.len() != expected_size {
            anyhow::bail!("Frame size mismatch: expected {}, got {}", expected_size, rgba_frame.len());
        }

        // Create RGBA source frame for swscale
        let mut rgba_src_frame = ffmpeg_next::frame::Video::empty();
        rgba_src_frame.set_width(self.width);
        rgba_src_frame.set_height(self.height);
        rgba_src_frame.set_format(ffmpeg_next::format::Pixel::RGBA);
        let ret = unsafe {
            ffmpeg_next::sys::av_frame_get_buffer(rgba_src_frame.as_mut_ptr(), 0)
        };
        if ret < 0 {
            anyhow::bail!("av_frame_get_buffer failed for RGBA frame: error code {}", ret);
        }

        // Copy RGBA data to source frame
        rgba_src_frame.data_mut(0).copy_from_slice(rgba_frame);

        // Create YUV destination frame
        let mut frame = ffmpeg_next::frame::Video::empty();
        frame.set_width(self.width);
        frame.set_height(self.height);
        frame.set_format(ffmpeg_next::format::Pixel::YUV420P);
        let ret = unsafe {
            ffmpeg_next::sys::av_frame_get_buffer(frame.as_mut_ptr(), 0)
        };
        if ret < 0 {
            anyhow::bail!("av_frame_get_buffer failed for YUV frame: error code {}", ret);
        }

        // Convert RGBA → YUV420P using cached swscale context
        self.scaler.run(&rgba_src_frame, &mut frame)
            .context("Failed to convert RGBA to YUV420P")?;

        // Set presentation timestamp (incrementing for each frame)
        frame.set_pts(Some(self.frame_count));
        self.frame_count += 1;

        // Force keyframe if requested (for new client connections)
        if self.force_keyframe_next {
            frame.set_kind(ffmpeg_next::util::picture::Type::I); // Force I-frame (keyframe)
            self.force_keyframe_next = false;
        }

        // Encode
        self.encoder.send_frame(&frame)
            .context("Failed to send frame to encoder")?;

        let mut packet = ffmpeg_next::Packet::empty();
        let mut encoded = Vec::new();

        // Receive all available packets
        while self.encoder.receive_packet(&mut packet).is_ok() {
            encoded.extend_from_slice(packet.data().unwrap_or(&[]));
        }

        Ok(encoded)
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

        let server_config = ServerConfig::with_single_cert(vec![cert_der], key_der)
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

/// State for Wayland event handling during screencopy
struct CaptureState {
    frame_ready: bool,
    frame_failed: bool,
    buffer_info: Option<(u32, u32, u32, u32)>, // format, width, height, stride
}

impl CaptureState {
    fn new() -> Self {
        Self {
            frame_ready: false,
            frame_failed: false,
            buffer_info: None,
        }
    }
}

/// Basic state for registry
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
        // Minimal handler
    }
}

// Screencopy frame event handler
impl Dispatch<ZwlrScreencopyFrameV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        _frame: &ZwlrScreencopyFrameV1,
        event: <ZwlrScreencopyFrameV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            FrameEvent::Buffer { format, width, height, stride } => {
                let format_u32: u32 = format.into();
                tracing::debug!("Buffer: {}x{}, stride={}, format={}", width, height, stride, format_u32);
                state.buffer_info = Some((format_u32, width, height, stride));
            }
            FrameEvent::BufferDone => {
                tracing::debug!("Buffer done - ready to copy");
            }
            FrameEvent::Ready { .. } => {
                tracing::debug!("Frame ready!");
                state.frame_ready = true;
            }
            FrameEvent::Failed => {
                tracing::error!("Screencopy failed");
                state.frame_failed = true;
            }
            _ => {}
        }
    }
}

mm_warp_common::wayland_dispatch_noop!(CaptureState;
    wl_shm::WlShm,
    wl_buffer::WlBuffer,
    wl_shm_pool::WlShmPool,
    wl_output::WlOutput,
    ZwlrScreencopyManagerV1,
);

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for CaptureState {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
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

    /// Capture a single frame from the first display using wlr-screencopy
    /// Returns raw RGBA buffer
    pub fn capture_frame(&mut self) -> Result<Vec<u8>> {
        // Initialize registry with CaptureState
        let (globals, mut event_queue) = registry_queue_init::<CaptureState>(&self.connection)
            .context("Failed to initialize registry")?;

        let qh = event_queue.handle();

        // Bind required globals
        let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ())
            .context("wl_shm not available")?;

        let screencopy_mgr: ZwlrScreencopyManagerV1 = globals.bind(&qh, 1..=3, ())
            .context("zwlr_screencopy_manager_v1 not available - compositor doesn't support screencopy")?;

        // Get first output
        let output: wl_output::WlOutput = globals.bind(&qh, 1..=4, ())
            .context("No wl_output available")?;

        tracing::info!("Bound screencopy manager and output");

        // Create shared memory buffer (hardcoded 1920x1080 RGBA for now)
        let width = 1920;
        let height = 1080;
        let stride = width * 4; // RGBA
        let size = (stride * height) as usize;

        tracing::debug!("Creating shm buffer: {}x{}, size={}", width, height, size);

        let (fd, mmap) = mm_warp_common::buffer::create_memfd_mmap("wl_shm", size)?;

        // Create wl_shm_pool and buffer
        let pool = shm.create_pool(fd.as_fd(), size as i32, &qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            &qh,
            (),
        );

        tracing::debug!("Created shm buffer");

        // Request screencopy (0 = no cursor overlay)
        let frame = screencopy_mgr.capture_output(0, &output, &qh, ());

        tracing::debug!("Requested screencopy");

        // Create state
        let mut state = CaptureState::new();

        // Event loop - wait for buffer info
        while state.buffer_info.is_none() && !state.frame_failed {
            event_queue.blocking_dispatch(&mut state)
                .context("Failed to dispatch events")?;
        }

        if state.frame_failed {
            anyhow::bail!("Screencopy failed");
        }

        // Copy frame to buffer
        frame.copy(&buffer);
        tracing::debug!("Issued copy request");

        // Wait for frame ready
        while !state.frame_ready && !state.frame_failed {
            event_queue.blocking_dispatch(&mut state)
                .context("Failed to dispatch events")?;
        }

        if state.frame_failed {
            anyhow::bail!("Screencopy failed during capture");
        }

        tracing::info!("Frame captured successfully");

        // Copy from mmap to output buffer (convert ARGB to RGBA)
        let mut rgba_buffer = vec![0u8; size];
        mm_warp_common::pixel::argb8888_to_rgba(mmap.as_ref(), &mut rgba_buffer, width, height);

        Ok(rgba_buffer)
    }
}

impl capture::FrameSource for WaylandConnection {
    fn capture_frame(&mut self) -> Result<Vec<u8>> {
        self.capture_frame()
    }

    fn resolution(&self) -> mm_warp_common::Resolution {
        mm_warp_common::Resolution::new(1920, 1080) // hardcoded in capture_frame
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
