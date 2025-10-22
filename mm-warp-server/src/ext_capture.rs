// ext-image-copy-capture-v1 implementation for COSMIC and newer compositors
// Based on wl-screenrec: https://github.com/russelltg/wl-screenrec

use anyhow::{Context, Result};
use std::os::fd::{AsFd, OwnedFd};
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
use nix::sys::memfd;
use nix::unistd::ftruncate;

/// Simplified capture using ext-image-copy-capture-v1
/// For mm-warp we just need RGB frames in memory (no GPU encoding)
pub struct ExtCapture {
    connection: Connection,
    session: ExtImageCopyCaptureSessionV1,
    shm: wl_shm::WlShm,
    pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
    mmap: MmapMut,
    width: u32,
    height: u32,
    refresh_rate: u32, // Monitor refresh rate in Hz
}

/// State for event handling during capture
struct CaptureState {
    frame_ready: bool,
    frame_failed: bool,
    width: Option<u32>,
    height: Option<u32>,
    shm_format: Option<u32>,
    session: Option<ExtImageCopyCaptureSessionV1>,
    buffer: Option<wl_buffer::WlBuffer>,
    mmap: Option<MmapMut>,
    logical_width: Option<i32>,
    logical_height: Option<i32>,
}

impl CaptureState {
    fn new() -> Self {
        Self {
            frame_ready: false,
            frame_failed: false,
            width: None,
            height: None,
            shm_format: None,
            session: None,
            buffer: None,
            mmap: None,
            logical_width: None,
            logical_height: None,
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
        state.session = Some(session.clone());

        while state.width.is_none() && !state.frame_failed {
            event_queue.blocking_dispatch(&mut state)
                .context("Failed to dispatch events")?;
        }

        if state.frame_failed {
            anyhow::bail!("Capture session failed during init");
        }

        let width = state.width.context("No width received")?;
        let height = state.height.context("No height received")?;
        let refresh_hz = 60; // Default to 60 Hz (TODO: query from wl_output Mode event)

        tracing::info!("Buffer size: {}x{} @ {} Hz", width, height, refresh_hz);

        // Create shared memory buffer ONCE
        let stride = width * 4;
        let size = (stride * height) as usize;

        let fd = memfd::memfd_create(
            std::ffi::CStr::from_bytes_with_nul(b"ext_cap\0").unwrap(),
            memfd::MemFdCreateFlag::MFD_CLOEXEC,
        ).context("Failed to create memfd")?;

        ftruncate(&fd, size as i64).context("Failed to truncate memfd")?;

        let mmap = unsafe {
            MmapMut::map_mut(&fd).context("Failed to mmap")?
        };

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
            connection,
            session,
            shm,
            pool,
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
        // Need queue handle for creating frame
        let (_, mut eq) = registry_queue_init::<CaptureState>(&self.connection)?;
        let qh = eq.handle();

        // Create and capture frame (reusing session and buffer)
        let frame = self.session.create_frame(&qh, ());
        frame.attach_buffer(&self.buffer);
        frame.damage_buffer(0, 0, self.width as i32, self.height as i32);
        frame.capture();

        // Wait for frame ready (minimal event loop)
        let mut state = CaptureState::new();
        while !state.frame_ready && !state.frame_failed {
            eq.blocking_dispatch(&mut state)?;
        }

        if state.frame_failed {
            anyhow::bail!("Frame capture failed");
        }

        // Copy from mmap directly (ABGR8888 is already RGBA in little-endian!)
        let size = (self.width * self.height * 4) as usize;
        let rgba_buffer = self.mmap.as_ref()[..size].to_vec();

        Ok(rgba_buffer)
    }

    /// Get the monitor's refresh rate in Hz
    pub fn refresh_rate(&self) -> u32 {
        self.refresh_rate
    }
}

// Minimal dispatch implementations (required by Wayland)
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for CaptureState {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ExtOutputImageCaptureSourceManagerV1, ()> for CaptureState {
    fn event(_: &mut Self, _: &ExtOutputImageCaptureSourceManagerV1, _: <ExtOutputImageCaptureSourceManagerV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ExtImageCopyCaptureManagerV1, ()> for CaptureState {
    fn event(_: &mut Self, _: &ExtImageCopyCaptureManagerV1, _: <ExtImageCopyCaptureManagerV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ExtImageCaptureSourceV1, ()> for CaptureState {
    fn event(_: &mut Self, _: &ExtImageCaptureSourceV1, _: <ExtImageCaptureSourceV1 as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

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
        // Capture logical size (accounts for scaling)
        if let wl_output::Event::Geometry { .. } = event {
            // Geometry gives physical info, we want logical
        }
        if let wl_output::Event::Mode { width, height, .. } = event {
            tracing::info!("Output mode (logical): {}x{}", width, height);
            state.logical_width = Some(width);
            state.logical_height = Some(height);
        }
    }
}

impl Dispatch<wl_shm::WlShm, ()> for CaptureState {
    fn event(_: &mut Self, _: &wl_shm::WlShm, _: wl_shm::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for CaptureState {
    fn event(_: &mut Self, _: &wl_shm_pool::WlShmPool, _: wl_shm_pool::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_buffer::WlBuffer, ()> for CaptureState {
    fn event(_: &mut Self, _: &wl_buffer::WlBuffer, _: wl_buffer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

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
