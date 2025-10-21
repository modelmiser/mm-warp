// ext-image-copy-capture-v1 implementation for COSMIC and newer compositors
// Based on wl-screenrec: https://github.com/russelltg/wl-screenrec

use anyhow::{Context, Result};
use wayland_client::{Connection, Dispatch, QueueHandle, Proxy};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry, wl_output};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::{ExtImageCopyCaptureFrameV1, Event as FrameEvent},
    ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
};

/// Simplified capture using ext-image-copy-capture-v1
/// For mm-warp we just need RGB frames in memory (no GPU encoding)
pub struct ExtCapture {
    connection: Connection,
}

/// State for event handling
struct CaptureState {
    frame_ready: bool,
    frame_failed: bool,
    width: Option<u32>,
    height: Option<u32>,
}

impl CaptureState {
    fn new() -> Self {
        Self {
            frame_ready: false,
            frame_failed: false,
            width: None,
            height: None,
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

    /// Capture a single frame (stub for now - will implement next)
    pub fn capture_frame(&mut self) -> Result<Vec<u8>> {
        tracing::info!("ext_capture::capture_frame called (stub)");

        // TODO: Implement actual capture
        // For now, return empty buffer to prove module works
        let width = 1920;
        let height = 1080;
        Ok(vec![0u8; width * height * 4])
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
                tracing::debug!("BufferSize: {}x{}", width, height);
                state.width = Some(width);
                state.height = Some(height);
            }
            ext_image_copy_capture_session_v1::Event::ShmFormat { .. } => {}
            ext_image_copy_capture_session_v1::Event::DmabufDevice { .. } => {}
            ext_image_copy_capture_session_v1::Event::DmabufFormat { .. } => {}
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
