pub mod input_event;
pub mod pixel;
pub mod buffer;
pub mod stats;

/// Generate empty Dispatch impls for Wayland protocol objects that don't
/// emit events we care about. Eliminates boilerplate across capture/display files.
///
/// Usage: `wayland_dispatch_noop!(State; wl_shm::WlShm, wl_buffer::WlBuffer, ...);`
#[macro_export]
macro_rules! wayland_dispatch_noop {
    ($state:ty; $($proto:ty),+ $(,)?) => {
        $(
            impl wayland_client::Dispatch<$proto, ()> for $state {
                fn event(
                    _: &mut Self,
                    _: &$proto,
                    _: <$proto as wayland_client::Proxy>::Event,
                    _: &(),
                    _: &wayland_client::Connection,
                    _: &wayland_client::QueueHandle<Self>,
                ) {}
            }
        )+
    };
}

pub use input_event::InputEvent;

/// Pixel dimensions propagated from capture → encoder → client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Stride in bytes (4 bytes per RGBA pixel).
    pub fn stride(&self) -> u32 {
        self.width * 4
    }

    /// Total buffer size in bytes for RGBA data.
    pub fn buffer_size(&self) -> usize {
        (self.stride() as usize) * (self.height as usize)
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

/// Stream metadata sent by server on the first unidirectional stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamMetadata {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl StreamMetadata {
    pub const SIZE: usize = 13; // 1 version + 4 width + 4 height + 4 fps
    const VERSION: u8 = 1;

    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        Self { width, height, fps }
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0] = Self::VERSION;
        buf[1..5].copy_from_slice(&self.width.to_be_bytes());
        buf[5..9].copy_from_slice(&self.height.to_be_bytes());
        buf[9..13].copy_from_slice(&self.fps.to_be_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        if bytes.len() < Self::SIZE {
            anyhow::bail!("StreamMetadata too short: {} bytes (need {})", bytes.len(), Self::SIZE);
        }
        if bytes[0] != Self::VERSION {
            anyhow::bail!("Unknown StreamMetadata version: {}", bytes[0]);
        }
        let width = u32::from_be_bytes(bytes[1..5].try_into()
            .map_err(|_| anyhow::anyhow!("StreamMetadata: invalid width bytes"))?);
        let height = u32::from_be_bytes(bytes[5..9].try_into()
            .map_err(|_| anyhow::anyhow!("StreamMetadata: invalid height bytes"))?);
        let fps = u32::from_be_bytes(bytes[9..13].try_into()
            .map_err(|_| anyhow::anyhow!("StreamMetadata: invalid fps bytes"))?);
        Ok(Self { width, height, fps })
    }
}

/// Get the mm-warp config directory (~/.config/mm-warp/).
pub fn config_dir() -> std::path::PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .expect("Neither XDG_CONFIG_HOME nor HOME is set — cannot determine config directory");
            std::path::PathBuf::from(home).join(".config")
        })
        .join("mm-warp")
}

/// Compute SHA-256 fingerprint of DER bytes as hex string.
pub fn cert_fingerprint(der: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    let hash = Sha256::digest(der);
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_round_trip() {
        let meta = StreamMetadata::new(3840, 2160, 60);
        let bytes = meta.to_bytes();
        let decoded = StreamMetadata::from_bytes(&bytes).unwrap();
        assert_eq!(meta, decoded);
    }

    #[test]
    fn metadata_too_short() {
        let result = StreamMetadata::from_bytes(&[1, 0, 0, 0]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    #[test]
    fn metadata_wrong_version() {
        let result = StreamMetadata::from_bytes(&[0; 13]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown"));
    }

    #[test]
    fn metadata_future_version() {
        let result = StreamMetadata::from_bytes(&[255; 13]);
        assert!(result.is_err());
    }

    #[test]
    fn resolution_buffer_size_4k() {
        let res = Resolution::new(3840, 2160);
        assert_eq!(res.stride(), 15360);
        assert_eq!(res.buffer_size(), 33_177_600);
    }

    #[test]
    fn resolution_buffer_size_max() {
        // 16384x16384 = max allowed by client validation
        let res = Resolution::new(16384, 16384);
        assert_eq!(res.buffer_size(), 1_073_741_824); // ~1GB
    }

    #[test]
    fn cert_fingerprint_length() {
        let fp = cert_fingerprint(b"test cert data");
        assert_eq!(fp.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn cert_fingerprint_deterministic() {
        let fp1 = cert_fingerprint(b"same data");
        let fp2 = cert_fingerprint(b"same data");
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn cert_fingerprint_different() {
        let fp1 = cert_fingerprint(b"data a");
        let fp2 = cert_fingerprint(b"data b");
        assert_ne!(fp1, fp2);
    }
}
