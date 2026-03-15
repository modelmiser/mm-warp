// Wayland window for displaying received frames — double-buffered
use anyhow::{Context, Result};
use std::os::fd::AsFd;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_compositor, wl_surface, wl_shm, wl_shm_pool, wl_buffer, wl_registry, wl_seat, wl_pointer, wl_keyboard};
use wayland_protocols::xdg::shell::client::{xdg_wm_base, xdg_surface, xdg_toplevel};
use wayland_protocols::wp::viewporter::client::{wp_viewporter, wp_viewport};
use memmap2::MmapMut;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct WaylandDisplay {
    connection: Connection,
    surface: wl_surface::WlSurface,
    _xdg_surface: xdg_surface::XdgSurface,
    _xdg_toplevel: xdg_toplevel::XdgToplevel,
    _viewport: wp_viewport::WpViewport,
    _shm: wl_shm::WlShm,
    _pools: [wl_shm_pool::WlShmPool; 2],
    buffers: [wl_buffer::WlBuffer; 2],
    mmaps: [MmapMut; 2],
    current_buf: usize,
    buffer_released: [Arc<AtomicBool>; 2],
    buffer_width: u32,
    buffer_height: u32,
    pending_events: Arc<Mutex<Vec<crate::InputEvent>>>,
    event_queue: wayland_client::EventQueue<State>,
    state: State,
    _keyboard: wl_keyboard::WlKeyboard,
    _pointer: wl_pointer::WlPointer,
}

/// Shared state for Wayland event dispatch.
/// Holds input event collection and buffer release tracking.
struct State {
    pending_events: Arc<Mutex<Vec<crate::InputEvent>>>,
    viewport_scale: u32,
    /// Per-buffer release flags. Set to true when compositor sends wl_buffer.release.
    buffer_released: [Arc<AtomicBool>; 2],
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

mm_warp_common::wayland_dispatch_noop!(State; wl_seat::WlSeat);

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(state: &mut Self, _: &wl_pointer::WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
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
                let axis_id = match axis {
                    WEnum::Value(wl_pointer::Axis::VerticalScroll) => 0u32,
                    WEnum::Value(wl_pointer::Axis::HorizontalScroll) => 1u32,
                    _ => return,
                };
                let steps = if value.abs() > 0.0 { (value / 15.0).round() as i32 } else { 0 };
                if steps != 0 {
                    let mut events = state.pending_events.lock().unwrap();
                    events.push(crate::InputEvent::MouseScroll { axis: axis_id, value: -steps });
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(state: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_keyboard::Event::Key { key, state: key_state, .. } = event {
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
    }
}

mm_warp_common::wayland_dispatch_noop!(State;
    wl_compositor::WlCompositor,
    wl_surface::WlSurface,
    wl_shm::WlShm,
    wl_shm_pool::WlShmPool,
);

/// Track wl_buffer.release events for double-buffering.
/// Each buffer is created with its index (0 or 1) as dispatch data.
impl Dispatch<wl_buffer::WlBuffer, usize> for State {
    fn event(
        state: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        data: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            if *data < 2 {
                state.buffer_released[*data].store(true, Ordering::Release);
            }
        }
    }
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
        let buffer_released = [
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicBool::new(true)),
        ];

        let viewport_scale = if width > 2560 { 2u32 } else { 1u32 };

        let mut state = State {
            pending_events: pending_events.clone(),
            viewport_scale,
            buffer_released: [buffer_released[0].clone(), buffer_released[1].clone()],
        };

        let (globals, mut event_queue) = registry_queue_init::<State>(&connection)
            .context("Failed to initialize registry")?;
        let qh = event_queue.handle();

        let compositor: wl_compositor::WlCompositor = globals.bind(&qh, 1..=1, ()).context("wl_compositor not available")?;
        let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).context("wl_shm not available")?;
        let wm_base: xdg_wm_base::XdgWmBase = globals.bind(&qh, 1..=1, ()).context("xdg_wm_base not available")?;
        let viewporter: wp_viewporter::WpViewporter = globals.bind(&qh, 1..=1, ()).context("wp_viewporter not available")?;
        let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=1, ()).context("wl_seat not available")?;

        let keyboard = seat.get_keyboard(&qh, ());
        let pointer = seat.get_pointer(&qh, ());

        let surface = compositor.create_surface(&qh, ());
        let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
        let xdg_toplevel = xdg_surface.get_toplevel(&qh, ());

        let viewport = viewporter.get_viewport(&surface, &qh, ());

        let win_w = (width / viewport_scale) as i32;
        let win_h = (height / viewport_scale) as i32;
        viewport.set_destination(win_w, win_h);

        xdg_toplevel.set_title("mm-warp - Remote Desktop".to_string());
        xdg_surface.set_window_geometry(0, 0, win_w, win_h);

        // Double-buffering: two separate memfds, pools, and buffers
        let stride = width * 4;
        let size = (stride as usize) * (height as usize);

        let (fd0, mmap0) = mm_warp_common::buffer::create_memfd_mmap("display0", size)?;
        let (fd1, mmap1) = mm_warp_common::buffer::create_memfd_mmap("display1", size)?;

        let pool0 = shm.create_pool(fd0.as_fd(), size as i32, &qh, ());
        let pool1 = shm.create_pool(fd1.as_fd(), size as i32, &qh, ());

        // Abgr8888 = RGBA bytes on LE — no conversion needed
        let buffer0 = pool0.create_buffer(0, width as i32, height as i32, stride as i32,
            wl_shm::Format::Abgr8888, &qh, 0usize);
        let buffer1 = pool1.create_buffer(0, width as i32, height as i32, stride as i32,
            wl_shm::Format::Abgr8888, &qh, 1usize);

        surface.commit();
        event_queue.roundtrip(&mut state).context("Failed to roundtrip")?;

        Ok(Self {
            connection,
            surface,
            _xdg_surface: xdg_surface,
            _xdg_toplevel: xdg_toplevel,
            _viewport: viewport,
            _shm: shm,
            _pools: [pool0, pool1],
            buffers: [buffer0, buffer1],
            mmaps: [mmap0, mmap1],
            current_buf: 0,
            buffer_released: [buffer_released[0].clone(), buffer_released[1].clone()],
            buffer_width: width,
            buffer_height: height,
            pending_events: pending_events.clone(),
            event_queue,
            state: State {
                pending_events: pending_events.clone(),
                viewport_scale,
                buffer_released: [buffer_released[0].clone(), buffer_released[1].clone()],
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

        // Dispatch pending Wayland events (processes wl_buffer.release from previous frame)
        let _ = self.event_queue.dispatch_pending(&mut self.state);

        let mut idx = self.current_buf;

        // Check if the target buffer has been released by the compositor.
        // If not, try the other buffer. If neither is free, proceed anyway
        // (same behavior as single-buffering — compositor is slow).
        if !self.buffer_released[idx].load(Ordering::Acquire) {
            let other = 1 - idx;
            if self.buffer_released[other].load(Ordering::Acquire) {
                idx = other;
            } else {
                // Both buffers held by compositor — drop frame to avoid protocol violation.
                // This is consistent with the server-side frame-dropping strategy.
                tracing::debug!("Both display buffers busy, dropping frame");
                return Ok(());
            }
        }

        // Write RGBA data directly to the selected buffer's mmap (Abgr8888 = RGBA on LE)
        self.mmaps[idx].as_mut()[..size].copy_from_slice(rgba_data);

        // Attach and commit
        self.surface.attach(Some(&self.buffers[idx]), 0, 0);
        self.surface.damage_buffer(0, 0, self.buffer_width as i32, self.buffer_height as i32);
        self.surface.commit();
        self.connection.flush().context("Failed to flush Wayland connection")?;

        // Mark this buffer as in-use by the compositor
        self.buffer_released[idx].store(false, Ordering::Release);

        // Swap to the other buffer for the next frame
        self.current_buf = 1 - idx;

        Ok(())
    }

    /// Poll and return any pending input events.
    /// Uses dispatch_pending (non-blocking) — release events and input events
    /// that arrived since the last call are processed without waiting.
    pub fn poll_input_events(&mut self) -> Vec<crate::InputEvent> {
        let _ = self.connection.flush();
        // Non-blocking: process events already in the queue (including wl_buffer.release)
        let _ = self.event_queue.dispatch_pending(&mut self.state);
        // Also read any new events from the socket without blocking
        if let Some(guard) = self.event_queue.prepare_read() {
            let _ = guard.read();
            let _ = self.event_queue.dispatch_pending(&mut self.state);
        }

        let mut events = self.pending_events.lock().unwrap();
        let result = events.clone();
        events.clear();
        result
    }
}
