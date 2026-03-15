// ext-image-copy-capture-v1 implementation for COSMIC and newer compositors
// Based on wl-screenrec: https://github.com/russelltg/wl-screenrec

use anyhow::{Context, Result};
use std::os::fd::AsFd;
use wayland_client::{Connection, Dispatch, QueueHandle, Proxy};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry, wl_output, wl_shm, wl_buffer, wl_shm_pool};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::{ExtImageCopyCaptureFrameV1, Event as FrameEvent},
    ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
};
use memmap2::MmapMut;

/// Simplified capture using ext-image-copy-capture-v1
/// For mm-warp we just need RGB frames in memory (no GPU encoding)
pub struct ExtCapture {
    _connection: Connection,        // Must stay alive for Wayland protocol
    event_queue: wayland_client::EventQueue<CaptureState>,
    session: ExtImageCopyCaptureSessionV1,
    _shm: wl_shm::WlShm,
    _pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
    mmap: MmapMut,
    width: u32,
    height: u32,
    refresh_rate: u32,
}

/// State for event handling during capture
struct CaptureState {
    frame_ready: bool,
    frame_failed: bool,
    width: Option<u32>,
    height: Option<u32>,
    shm_format: Option<u32>,
    refresh_rate: Option<u32>,
}

impl CaptureState {
    fn new() -> Self {
        Self {
            frame_ready: false,
            frame_failed: false,
            width: None,
            height: None,
            shm_format: None,
            refresh_rate: None,
        }
    }
}

impl ExtCapture {
    /// Create new ext capture instance with reusable resources
    pub fn new() -> Result<Self> {
        let connection = Connection::connect_to_env()
            .context("Failed to connect to Wayland (ext-image-copy-capture)")?;

        // Initialize registry ONCE
        let (globals, mut event_queue) = registry_queue_init::<CaptureState>(&connection)
            .context("Failed to initialize registry")?;

        let qh = event_queue.handle();

        // Bind required managers ONCE
        let source_mgr: ExtOutputImageCaptureSourceManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .context("ext_output_image_capture_source_manager_v1 not available")?;

        let copy_mgr: ExtImageCopyCaptureManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .context("ext_image_copy_capture_manager_v1 not available")?;

        let shm: wl_shm::WlShm = globals
            .bind(&qh, 1..=1, ())
            .context("wl_shm not available")?;

        let output: wl_output::WlOutput = globals
            .bind(&qh, 1..=1, ())
            .context("No output available")?;

        // Create capture source and session ONCE
        let source = source_mgr.create_source(&output, &qh, ());
        // Use PaintCursors to include cursor in capture
        let session = copy_mgr.create_session(&source, Options::PaintCursors, &qh, ());

        // Get buffer constraints
        let mut state = CaptureState::new();

        while state.width.is_none() && !state.frame_failed {
            event_queue.blocking_dispatch(&mut state)
                .context("Failed to dispatch events")?;
        }

        if state.frame_failed {
            anyhow::bail!("Capture session failed during init");
        }

        let width = state.width.context("No width received")?;
        let height = state.height.context("No height received")?;
        let refresh_hz = state.refresh_rate.unwrap_or(60); // Default to 60 Hz if not reported

        tracing::info!("Buffer size: {}x{} @ {} Hz", width, height, refresh_hz);

        // Create shared memory buffer ONCE
        let stride = width * 4;
        let size = (stride * height) as usize;

        let (fd, mmap) = mm_warp_common::buffer::create_memfd_mmap("ext_cap", size)?;

        let pool = shm.create_pool(fd.as_fd(), size as i32, &qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Abgr8888,
            &qh,
            (),
        );

        Ok(Self {
            _connection: connection,
            event_queue,
            session,
            _shm: shm,
            _pool: pool,
            buffer,
            mmap,
            width,
            height,
            refresh_rate: refresh_hz,
        })
    }

    /// Check if ext-image-copy-capture is available
    pub fn is_available() -> bool {
        if let Ok(conn) = Connection::connect_to_env() {
            if let Ok((globals, _)) = registry_queue_init::<CaptureState>(&conn) {
                // Check for both required managers
                let has_source = globals.contents().with_list(|list| {
                    list.iter().any(|g| g.interface == "ext_output_image_capture_source_manager_v1")
                });
                let has_copy = globals.contents().with_list(|list| {
                    list.iter().any(|g| g.interface == "ext_image_copy_capture_manager_v1")
                });
                return has_source && has_copy;
            }
        }
        false
    }

    /// Capture a single frame using ext-image-copy-capture (optimized)
    pub fn capture_frame(&mut self) -> Result<Vec<u8>> {
        let qh = self.event_queue.handle();

        // Create and capture frame (reusing session, buffer, and event queue)
        let frame = self.session.create_frame(&qh, ());
        frame.attach_buffer(&self.buffer);
        frame.damage_buffer(0, 0, self.width as i32, self.height as i32);
        frame.capture();

        // Wait for frame ready using persistent event queue
        let mut state = CaptureState::new();
        while !state.frame_ready && !state.frame_failed {
            self.event_queue.blocking_dispatch(&mut state)?;
        }

        if state.frame_failed {
            anyhow::bail!("Frame capture failed");
        }

        // No pixel format conversion needed. From wayland.xml:
        //
        //   abgr8888: "32-bit ABGR format, [31:0] A:B:G:R 8:8:8:8 little endian"
        //
        // The name describes the 32-bit word layout from MSB to LSB: A=bits[31:24],
        // B=bits[23:16], G=bits[15:8], R=bits[7:0]. As a 32-bit integer: 0xAABBGGRR.
        // On little-endian (x86), this integer is stored in memory as bytes:
        //   [byte0=RR, byte1=GG, byte2=BB, byte3=AA] = RGBA byte order.
        //
        // So Abgr8888 in shared memory IS RGBA bytes on little-endian. No swizzle.
        //
        // Compare with Argb8888 (used by wlr-screencopy path in lib.rs):
        //   argb8888 word = 0xAARRGGBB → LE bytes [BB,GG,RR,AA] = BGRA → needs conversion.
        //
        // See: wayland.xml wl_shm.format enum, drm_fourcc.h naming convention.
        // Verified by: cargo run --bin test_pixel_format
        let size = (self.width * self.height * 4) as usize;
        let rgba_buffer = self.mmap.as_ref()[..size].to_vec();

        Ok(rgba_buffer)
    }

    /// Get the monitor's refresh rate in Hz
    pub fn refresh_rate(&self) -> u32 {
        self.refresh_rate
    }
}

impl crate::capture::FrameSource for ExtCapture {
    fn capture_frame(&mut self) -> anyhow::Result<Vec<u8>> {
        self.capture_frame()
    }

    fn resolution(&self) -> mm_warp_common::Resolution {
        mm_warp_common::Resolution::new(self.width, self.height)
    }
}

// Minimal dispatch implementations (required by Wayland)
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for CaptureState {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

mm_warp_common::wayland_dispatch_noop!(CaptureState;
    ExtOutputImageCaptureSourceManagerV1,
    ExtImageCopyCaptureManagerV1,
    ExtImageCaptureSourceV1,
);

impl Dispatch<ExtImageCopyCaptureSessionV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureSessionV1,
        event: <ExtImageCopyCaptureSessionV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_image_copy_capture_session_v1::Event::BufferSize { width, height } => {
                tracing::info!("BufferSize: {}x{}", width, height);
                state.width = Some(width);
                state.height = Some(height);
            }
            ext_image_copy_capture_session_v1::Event::ShmFormat { format } => {
                tracing::info!("ShmFormat supported: {:?}", format);
                state.shm_format = Some(format.into());
            }
            ext_image_copy_capture_session_v1::Event::DmabufDevice { .. } => {
                tracing::info!("DmabufDevice event (ignoring - using shm)");
            }
            ext_image_copy_capture_session_v1::Event::DmabufFormat { .. } => {
                tracing::info!("DmabufFormat event (ignoring - using shm)");
            }
            ext_image_copy_capture_session_v1::Event::Done => {
                tracing::debug!("Session Done");
            }
            ext_image_copy_capture_session_v1::Event::Stopped => {
                state.frame_failed = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureFrameV1,
        event: <ExtImageCopyCaptureFrameV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            FrameEvent::Transform { .. } => {}
            FrameEvent::Damage { .. } => {}
            FrameEvent::PresentationTime { .. } => {}
            FrameEvent::Ready => {
                tracing::debug!("Frame Ready!");
                state.frame_ready = true;
            }
            FrameEvent::Failed { reason } => {
                tracing::error!("Frame Failed: {:?}", reason);
                state.frame_failed = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for CaptureState {
    fn event(state: &mut Self, _: &wl_output::WlOutput, event: wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_output::Event::Mode { flags, width, height, refresh } = event {
            use wayland_client::WEnum;
            let refresh_hz = ((refresh + 500) / 1000) as u32; // mHz to Hz (rounded)

            let is_current = match flags {
                WEnum::Value(f) => f.contains(wl_output::Mode::Current),
                WEnum::Unknown(_) => false,
            };

            if is_current {
                state.refresh_rate = Some(refresh_hz);
                tracing::info!("Output mode: {}x{} @ {} Hz", width, height, refresh_hz);
            }
        }
    }
}

mm_warp_common::wayland_dispatch_noop!(CaptureState;
    wl_shm::WlShm,
    wl_shm_pool::WlShmPool,
    wl_buffer::WlBuffer,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ext_available() {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            let is_avail = ExtCapture::is_available();
            println!("ext-image-copy-capture available: {}", is_avail);
        }
    }
}
