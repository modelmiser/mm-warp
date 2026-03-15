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
        (self.stride() * self.height) as usize
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}
