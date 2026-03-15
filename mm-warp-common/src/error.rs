/// Structured error types for mm-warp pipeline failures.
#[derive(Debug, thiserror::Error)]
pub enum WarpError {
    #[error("Screen capture disconnected: {0}")]
    CaptureDisconnected(String),

    #[error("Input injection failed: {0}")]
    InputInjectionFailed(String),

    #[error("Wayland connection lost: {0}")]
    WaylandLost(String),

    #[error("Codec error: {0}")]
    CodecError(String),

    #[error("Client disconnected")]
    ClientDisconnected,

    #[error("Resolution mismatch: capture={capture}, encoder={encoder}")]
    ResolutionMismatch {
        capture: crate::Resolution,
        encoder: crate::Resolution,
    },
}
