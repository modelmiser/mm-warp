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
    _xdg_surface: xdg_surface::XdgSurface,
    _xdg_toplevel: xdg_toplevel::XdgToplevel,
    _viewport: wp_viewport::WpViewport,
    _shm: wl_shm::WlShm,
    _pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
    mmap: MmapMut,
    buffer_width: u32,
    buffer_height: u32,
    pending_events: Arc<Mutex<Vec<crate::InputEvent>>>,
    event_queue: wayland_client::EventQueue<State>,
    state: State,
    _keyboard: wl_keyboard::WlKeyboard,
    _pointer: wl_pointer::WlPointer,
}

// State for input event collection — persisted across polls
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
            wl_pointer::Event::Axis { axis, value, .. } => {
                use wayland_client::WEnum;
                // axis: 0 = vertical scroll, 1 = horizontal scroll
                let axis_id = match axis {
                    WEnum::Value(wl_pointer::Axis::VerticalScroll) => 0u32,
                    WEnum::Value(wl_pointer::Axis::HorizontalScroll) => 1u32,
                    _ => return,
                };
                // Wayland scroll value is in surface-local coordinates.
                // Convert to discrete scroll steps (15 pixels per step is typical).
                let steps = if value.abs() > 0.0 { (value / 15.0).round() as i32 } else { 0 };
                if steps != 0 {
                    let mut events = state.pending_events.lock().unwrap();
                    // Negate: Wayland positive = scroll down, REL_WHEEL positive = scroll up
                    events.push(crate::InputEvent::MouseScroll { axis: axis_id, value: -steps });
                }
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

        // Auto-scale: if buffer > 2560 wide, use 2x viewport; otherwise 1x
        let viewport_scale = if width > 2560 { 2u32 } else { 1u32 };

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

        let seat: wl_seat::WlSeat = globals
            .bind(&qh, 1..=1, ())
            .context("wl_seat not available")?;

        let keyboard = seat.get_keyboard(&qh, ());
        let pointer = seat.get_pointer(&qh, ());

        let surface = compositor.create_surface(&qh, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
        let xdg_toplevel = xdg_surface.get_toplevel(&qh, ());

        let viewport = viewporter.get_viewport(&surface, &qh, ());

        // Set viewport destination based on scale
        let win_w = (width / viewport_scale) as i32;
        let win_h = (height / viewport_scale) as i32;
        viewport.set_destination(win_w, win_h);

        xdg_toplevel.set_title("mm-warp - Remote Desktop".to_string());
        xdg_surface.set_window_geometry(0, 0, win_w, win_h);

        // Create shared memory buffer
        let stride = width * 4;
        let size = (stride as usize) * (height as usize);

        let (fd, mmap) = mm_warp_common::buffer::create_memfd_mmap("display", size)?;

        let pool = shm.create_pool(fd.as_fd(), size as i32, &qh, ());
        // Use Abgr8888 which is RGBA byte order on little-endian (no conversion needed).
        // See ext_capture.rs for the full derivation from wayland.xml:
        //   Abgr8888 word = 0xAABBGGRR → LE bytes = [R, G, B, A] = RGBA
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Abgr8888,
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
            state: State {
                pending_events: pending_events.clone(),
                viewport_scale,
            },
            _keyboard: keyboard,
            _pointer: pointer,
        })
    }

    pub fn display_frame(&mut self, rgba_data: &[u8]) -> Result<()> {
        let size = (self.buffer_width as usize) * (self.buffer_height as usize) * 4;

        if rgba_data.len() != size {
            anyhow::bail!("Frame size mismatch: expected {}, got {}", size, rgba_data.len());
        }

        // Abgr8888 = RGBA bytes on LE — direct copy, no conversion needed
        self.mmap.as_mut()[..size].copy_from_slice(rgba_data);

        self.surface.attach(Some(&self.buffer), 0, 0);
        self.surface.damage_buffer(0, 0, self.buffer_width as i32, self.buffer_height as i32);
        self.surface.commit();

        self.connection.flush().context("Failed to flush Wayland connection")?;

        Ok(())
    }

    /// Poll and return any pending input events.
    /// Uses roundtrip() to read events from the compositor.
    pub fn poll_input_events(&mut self) -> Vec<crate::InputEvent> {
        let _ = self.connection.flush();

        // Dispatch using persistent state (preserves viewport_scale across calls)
        let _ = self.event_queue.roundtrip(&mut self.state);

        let mut events = self.pending_events.lock().unwrap();
        let result = events.clone();
        events.clear();
        result
    }
}
