use mm_warp_common::Resolution;

/// Trait for screen capture backends.
///
/// Implementors produce RGBA frame data from a display source.
pub trait FrameSource {
    /// Capture a single frame, returning raw RGBA pixel data.
    fn capture_frame(&mut self) -> anyhow::Result<Vec<u8>>;

    /// Resolution of captured frames.
    fn resolution(&self) -> Resolution;
}
