// Quick test to verify ext modules are available in wayland-protocols 0.32

#[allow(unused_imports)]
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
    ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1,
    ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
};

#[allow(unused_imports)]
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};

#[test]
fn test_ext_modules_available() {
    // If this compiles, the ext modules are available!
    println!("✅ ext-image-copy-capture-v1 modules are available in wayland-protocols 0.32");
}
