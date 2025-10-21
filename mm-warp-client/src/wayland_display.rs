// Wayland window for displaying received frames
use anyhow::{Context, Result};
use std::os::fd::AsFd;
use wayland_client::{Connection, Dispatch, QueueHandle, Proxy};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_compositor, wl_surface, wl_shm, wl_shm_pool, wl_buffer, wl_registry, wl_seat, wl_pointer, wl_keyboard};
use wayland_protocols::xdg::shell::client::{xdg_wm_base, xdg_surface, xdg_toplevel};
use wayland_protocols::wp::viewporter::client::{wp_viewporter, wp_viewport};
use memmap2::MmapMut;
use nix::sys::memfd;
use nix::unistd::ftruncate;
use std::sync::{Arc, Mutex};

pub struct WaylandDisplay {
    connection: Connection,
    surface: wl_surface::WlSurface,
    xdg_surface: xdg_surface::XdgSurface,
    _xdg_toplevel: xdg_toplevel::XdgToplevel,
    viewport: wp_viewport::WpViewport,
    shm: wl_shm::WlShm,
    pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
    mmap: MmapMut,
    buffer_width: u32,
    buffer_height: u32,
    pending_events: Arc<Mutex<Vec<crate::InputEvent>>>,
}

// State for input event collection
struct State {
    pending_events: Arc<Mutex<Vec<crate::InputEvent>>>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(state: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                let mut events = state.pending_events.lock().unwrap();
                events.push(crate::InputEvent::MouseMove {
                    x: surface_x as i32,
                    y: surface_y as i32,
                });
            }
            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                let pressed = btn_state == wl_pointer::ButtonState::Pressed;
                let mut events = state.pending_events.lock().unwrap();
                events.push(crate::InputEvent::MouseButton { button, pressed });
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Key { key, state: key_state, .. } => {
                let mut events = state.pending_events.lock().unwrap();
                match key_state {
                    wl_keyboard::KeyState::Pressed => {
                        events.push(crate::InputEvent::KeyPress { key });
                    }
                    wl_keyboard::KeyState::Released => {
                        events.push(crate::InputEvent::KeyRelease { key });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn event(_: &mut Self, _: &wl_compositor::WlCompositor, _: wl_compositor::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(_: &mut Self, _: &wl_surface::WlSurface, _: wl_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(_: &mut Self, _: &wl_shm::WlShm, _: wl_shm::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for State {
    fn event(_: &mut Self, _: &wl_shm_pool::WlShmPool, _: wl_shm_pool::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(_: &mut Self, _: &wl_buffer::WlBuffer, _: wl_buffer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(_: &mut Self, wm_base: &xdg_wm_base::XdgWmBase, event: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(_: &mut Self, xdg_surface: &xdg_surface::XdgSurface, event: xdg_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(_: &mut Self, _: &xdg_toplevel::XdgToplevel, _: xdg_toplevel::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wp_viewporter::WpViewporter, ()> for State {
    fn event(_: &mut Self, _: &wp_viewporter::WpViewporter, _: wp_viewporter::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wp_viewport::WpViewport, ()> for State {
    fn event(_: &mut Self, _: &wp_viewport::WpViewport, _: wp_viewport::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl WaylandDisplay {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let connection = Connection::connect_to_env()
            .context("Failed to connect to Wayland")?;

        let pending_events = Arc::new(Mutex::new(Vec::new()));

        let state = State {
            pending_events: pending_events.clone(),
        };

        // Can't use registry_queue_init with stateful State
        // Need manual registry setup for input events
        // For now, simplified: create without input (add later)

        let qh = _event_queue.handle();

        let compositor: wl_compositor::WlCompositor = globals
            .bind(&qh, 1..=1, ())
            .context("wl_compositor not available")?;

        let shm: wl_shm::WlShm = globals
            .bind(&qh, 1..=1, ())
            .context("wl_shm not available")?;

        let wm_base: xdg_wm_base::XdgWmBase = globals
            .bind(&qh, 1..=1, ())
            .context("xdg_wm_base not available")?;

        let viewporter: wp_viewporter::WpViewporter = globals
            .bind(&qh, 1..=1, ())
            .context("wp_viewporter not available")?;

        // Bind seat for input
        let seat: wl_seat::WlSeat = globals
            .bind(&qh, 1..=1, ())
            .context("wl_seat not available")?;

        // Get pointer and keyboard
        let pointer = seat.get_pointer(&qh, ());
        let keyboard = seat.get_keyboard(&qh, ());

        // Create surface and make it a window
        let surface = compositor.create_surface(&qh, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
        let xdg_toplevel = xdg_surface.get_toplevel(&qh, ());

        // Create viewport for scaling the buffer
        let viewport = viewporter.get_viewport(&surface, &qh, ());

        // Set viewport destination to half size (1920x1080 window for 4K buffer)
        viewport.set_destination((width / 2) as i32, (height / 2) as i32);

        // Set window title
        xdg_toplevel.set_title("mm-warp - Remote Desktop".to_string());

        // Set window geometry to match viewport
        xdg_surface.set_window_geometry(0, 0, (width / 2) as i32, (height / 2) as i32);

        // Create shared memory buffer ONCE (reuse for all frames)
        let stride = width * 4;
        let size = (stride * height) as usize;

        let fd = memfd::memfd_create(
            std::ffi::CStr::from_bytes_with_nul(b"display\0").unwrap(),
            memfd::MemFdCreateFlag::MFD_CLOEXEC,
        ).context("Failed to create memfd")?;

        ftruncate(&fd, size as i64).context("Failed to truncate memfd")?;

        let mmap = unsafe {
            MmapMut::map_mut(&fd).context("Failed to mmap")?
        };

        let pool = shm.create_pool(fd.as_fd(), size as i32, &qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            &qh,
            (),
        );

        // Initial commit to create the window
        surface.commit();

        Ok(Self {
            connection,
            surface,
            xdg_surface,
            _xdg_toplevel: xdg_toplevel,
            viewport,
            shm,
            pool,
            buffer,
            mmap,
            buffer_width: width,
            buffer_height: height,
            pending_events: pending_events.clone(),
        })
    }

    pub fn display_frame(&mut self, rgba_data: &[u8]) -> Result<()> {
        let size = (self.buffer_width * self.buffer_height * 4) as usize;

        if rgba_data.len() != size {
            anyhow::bail!("Frame size mismatch: expected {}, got {}", size, rgba_data.len());
        }

        // Reuse the existing mmap (no new allocation!)
        let mmap_slice = self.mmap.as_mut();

        // Copy RGBA data to mmap (convert to ARGB8888 for Wayland)
        for i in 0..(self.buffer_width * self.buffer_height) as usize {
            let idx = i * 4;
            // RGBA → ARGB8888 (little-endian: [B,G,R,A])
            mmap_slice[idx] = rgba_data[idx + 2];     // B
            mmap_slice[idx + 1] = rgba_data[idx + 1]; // G
            mmap_slice[idx + 2] = rgba_data[idx];     // R
            mmap_slice[idx + 3] = rgba_data[idx + 3]; // A
        }

        // Reuse existing buffer - just attach and commit (viewport handles scaling)
        self.surface.attach(Some(&self.buffer), 0, 0);
        self.surface.damage_buffer(0, 0, self.buffer_width as i32, self.buffer_height as i32);
        self.surface.commit();

        // Flush the connection
        self.connection.flush().context("Failed to flush Wayland connection")?;

        Ok(())
    }

    /// Poll and return any pending input events
    pub fn poll_input_events(&mut self) -> Vec<crate::InputEvent> {
        let mut events = self.pending_events.lock().unwrap();
        let result = events.clone();
        events.clear();
        result
    }
}
