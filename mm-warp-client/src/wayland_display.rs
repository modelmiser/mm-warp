// Wayland window for displaying received frames
use anyhow::{Context, Result};
use std::os::fd::AsFd;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_compositor, wl_surface, wl_shm, wl_shm_pool, wl_buffer, wl_registry, wl_seat, wl_pointer, wl_keyboard};
use wayland_protocols::xdg::shell::client::{xdg_wm_base, xdg_surface, xdg_toplevel};
use wayland_protocols::wp::viewporter::client::{wp_viewporter, wp_viewport};
use memmap2::MmapMut;
use std::sync::{Arc, Mutex};

pub struct WaylandDisplay {
    connection: Connection,
    surface: wl_surface::WlSurface,
    _xdg_surface: xdg_surface::XdgSurface,     // Must stay alive for window lifecycle
    _xdg_toplevel: xdg_toplevel::XdgToplevel,
    _viewport: wp_viewport::WpViewport,          // Must stay alive for scaling
    _shm: wl_shm::WlShm,
    _pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
    mmap: MmapMut,
    buffer_width: u32,
    buffer_height: u32,
    pending_events: Arc<Mutex<Vec<crate::InputEvent>>>,
    event_queue: wayland_client::EventQueue<State>,
    _keyboard: wl_keyboard::WlKeyboard,
    _pointer: wl_pointer::WlPointer,
}

// State for input event collection
struct State {
    pending_events: Arc<Mutex<Vec<crate::InputEvent>>>,
    viewport_scale: u32,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

mm_warp_common::wayland_dispatch_noop!(State; wl_seat::WlSeat);

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(state: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                // Surface coordinates are in viewport (window) space.
                // Scale by viewport factor to get buffer-space coordinates,
                // which match the server's screen resolution.
                let scale = state.viewport_scale as f64;
                let mut events = state.pending_events.lock().unwrap();
                events.push(crate::InputEvent::MouseMove {
                    x: (surface_x * scale) as i32,
                    y: (surface_y * scale) as i32,
                });
            }
            wl_pointer::Event::Button { button, state: btn_state, .. } => {
                use wayland_client::WEnum;
                let pressed = matches!(btn_state, WEnum::Value(wl_pointer::ButtonState::Pressed));
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
                use wayland_client::WEnum;
                let mut events = state.pending_events.lock().unwrap();
                match key_state {
                    WEnum::Value(wl_keyboard::KeyState::Pressed) => {
                        events.push(crate::InputEvent::KeyPress { key });
                    }
                    WEnum::Value(wl_keyboard::KeyState::Released) => {
                        events.push(crate::InputEvent::KeyRelease { key });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

mm_warp_common::wayland_dispatch_noop!(State;
    wl_compositor::WlCompositor,
    wl_surface::WlSurface,
    wl_shm::WlShm,
    wl_shm_pool::WlShmPool,
    wl_buffer::WlBuffer,
);

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

mm_warp_common::wayland_dispatch_noop!(State;
    xdg_toplevel::XdgToplevel,
    wp_viewporter::WpViewporter,
    wp_viewport::WpViewport,
);

impl WaylandDisplay {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        let connection = Connection::connect_to_env()
            .context("Failed to connect to Wayland")?;

        let pending_events = Arc::new(Mutex::new(Vec::new()));

        // Viewport maps the full buffer to half-size window.
        // Pointer events arrive in window coords; scale them back to buffer coords.
        let viewport_scale = 2u32;

        let mut state = State {
            pending_events: pending_events.clone(),
            viewport_scale,
        };

        // Initialize globals
        let (globals, mut event_queue) = registry_queue_init::<State>(&connection)
            .context("Failed to initialize registry")?;
        let qh = event_queue.handle();

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

        // Bind seat for input events
        let seat: wl_seat::WlSeat = globals
            .bind(&qh, 1..=1, ())
            .context("wl_seat not available")?;

        // Get keyboard and pointer from seat (must keep alive!)
        let keyboard = seat.get_keyboard(&qh, ());
        let pointer = seat.get_pointer(&qh, ());

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

        let (fd, mmap) = mm_warp_common::buffer::create_memfd_mmap("display", size)?;

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

        // Dispatch initial events
        event_queue.roundtrip(&mut state).context("Failed to roundtrip")?;

        Ok(Self {
            connection,
            surface,
            _xdg_surface: xdg_surface,
            _xdg_toplevel: xdg_toplevel,
            _viewport: viewport,
            _shm: shm,
            _pool: pool,
            buffer,
            mmap,
            buffer_width: width,
            buffer_height: height,
            pending_events: pending_events.clone(),
            event_queue,
            _keyboard: keyboard,
            _pointer: pointer,
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
        mm_warp_common::pixel::rgba_to_argb8888(rgba_data, mmap_slice, self.buffer_width, self.buffer_height);

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
        // Flush pending requests
        let _ = self.connection.flush();

        // Read from Wayland socket and dispatch events
        // roundtrip() is needed to actually read from socket (dispatch_pending doesn't!)
        let mut state = State {
            pending_events: self.pending_events.clone(),
            viewport_scale: 2,
        };
        let _ = self.event_queue.roundtrip(&mut state);

        // Return collected events
        let mut events = self.pending_events.lock().unwrap();
        let result = events.clone();
        events.clear();
        result
    }
}
