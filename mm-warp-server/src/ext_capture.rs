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
        }
    }
}

impl ExtCapture {
    /// Create new ext capture instance
    pub fn new() -> Result<Self> {
        let connection = Connection::connect_to_env()
            .context("Failed to connect to Wayland (ext-image-copy-capture)")?;

        Ok(Self { connection })
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

    /// Capture a single frame using ext-image-copy-capture
    pub fn capture_frame(&mut self) -> Result<Vec<u8>> {
        tracing::info!("Capturing frame via ext-image-copy-capture-v1");

        // Initialize registry
        let (globals, mut event_queue) = registry_queue_init::<CaptureState>(&self.connection)
            .context("Failed to initialize registry")?;

        let qh = event_queue.handle();

        // Bind required managers
        let source_mgr: ExtOutputImageCaptureSourceManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .context("ext_output_image_capture_source_manager_v1 not available")?;

        let copy_mgr: ExtImageCopyCaptureManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .context("ext_image_copy_capture_manager_v1 not available")?;

        let shm: wl_shm::WlShm = globals
            .bind(&qh, 1..=1, ())
            .context("wl_shm not available")?;

        // Get first output
        let output: wl_output::WlOutput = globals
            .bind(&qh, 1..=1, ())
            .context("No output available")?;

        tracing::debug!("Bound all required managers");

        // Create capture source from output
        let source = source_mgr.create_source(&output, &qh, ());

        // Create capture session
        let session = copy_mgr.create_session(&source, Options::empty(), &qh, ());

        // Create state
        let mut state = CaptureState::new();
        state.session = Some(session);

        // Dispatch events until we get buffer constraints
        tracing::debug!("Waiting for buffer constraints...");
        while state.width.is_none() && !state.frame_failed {
            event_queue.blocking_dispatch(&mut state)
                .context("Failed to dispatch events")?;
        }

        if state.frame_failed {
            anyhow::bail!("Capture session failed");
        }

        let width = state.width.context("No width received")?;
        let height = state.height.context("No height received")?;

        tracing::info!("Buffer size: {}x{}", width, height);

        // Create shared memory buffer
        let stride = width * 4; // RGBA
        let size = (stride * height) as usize;

        let fd = memfd::memfd_create(
            std::ffi::CStr::from_bytes_with_nul(b"ext_shm\0").unwrap(),
            memfd::MemFdCreateFlag::MFD_CLOEXEC,
        ).context("Failed to create memfd")?;

        ftruncate(&fd, size as i64).context("Failed to truncate memfd")?;

        let mmap = unsafe {
            MmapMut::map_mut(&fd).context("Failed to mmap")?
        };

        // Create wl_shm pool and buffer
        // Use Abgr8888 (COSMIC's preferred format)
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

        state.buffer = Some(buffer.clone());
        state.mmap = Some(mmap);

        tracing::debug!("Created shm buffer");

        // Create and capture frame
        let frame = state.session.as_ref().unwrap().create_frame(&qh, ());
        frame.attach_buffer(&buffer);
        frame.damage_buffer(0, 0, width as i32, height as i32);
        frame.capture();

        tracing::debug!("Issued capture request");

        // Wait for frame ready
        while !state.frame_ready && !state.frame_failed {
            event_queue.blocking_dispatch(&mut state)
                .context("Failed to dispatch events")?;
        }

        if state.frame_failed {
            anyhow::bail!("Frame capture failed");
        }

        tracing::info!("Frame captured successfully!");

        // Copy from mmap to output buffer (convert ABGR to RGBA)
        let mut rgba_buffer = vec![0u8; size];
        let mmap_data = state.mmap.as_ref().unwrap().as_ref();

        for i in 0..(width * height) as usize {
            let idx = i * 4;
            // ABGR8888: [R, G, B, A] in little-endian memory (already correct order!)
            // RGBA: [R, G, B, A]
            rgba_buffer[idx] = mmap_data[idx];         // R
            rgba_buffer[idx + 1] = mmap_data[idx + 1]; // G
            rgba_buffer[idx + 2] = mmap_data[idx + 2]; // B
            rgba_buffer[idx + 3] = mmap_data[idx + 3]; // A
        }

        Ok(rgba_buffer)
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
    fn event(_: &mut Self, _: &wl_output::WlOutput, _: wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
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
