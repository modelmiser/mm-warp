// Test to verify ext modules are available in wayland-protocols 0.32

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

fn main() {
    println!("✅ SUCCESS! ext-image-copy-capture-v1 modules ARE available in wayland-protocols 0.32");
    println!();
    println!("Available types:");
    println!("  - ExtImageCopyCaptureManagerV1");
    println!("  - ExtImageCopyCaptureSessionV1");
    println!("  - ExtImageCopyCaptureFrameV1");
    println!("  - ExtOutputImageCaptureSourceManagerV1");
    println!("  - ExtImageCaptureSourceV1");
    println!();
    println!("This means we can implement COSMIC support WITHOUT generating bindings!");
}
