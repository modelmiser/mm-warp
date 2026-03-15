/// Structured error types for mm-warp pipeline failures.
#[derive(Debug, thiserror::Error)]
pub enum WarpError {
    #[error("Client disconnected")]
    ClientDisconnected,
}
