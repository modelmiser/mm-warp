pub mod input_event;
pub mod pixel;
pub mod buffer;
pub mod stats;
pub mod error;

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
pub use error::WarpError;

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
/// Client reads this before entering the frame receive loop to auto-configure
/// decoder resolution and display window size.
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
