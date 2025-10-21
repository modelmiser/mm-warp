# Wayland Screencopy Implementation Plan

**Status**: Documented, ready to implement
**Complexity**: Medium (1-2 days)
**Priority**: HIGH (only remaining core feature)

---

## Implementation Pattern (from C examples)

### 1. Bind Screencopy Manager

During Wayland registry initialization:
```rust
// In registry global handler
if interface == "zwlr_screencopy_manager_v1" {
    let screencopy_mgr = registry.bind::<ZwlrScreencopyManagerV1, _, _>(
        name, version, &qh, ()
    );
    // Store in state
}
```

### 2. Create Shared Memory Buffer

```rust
use std::os::unix::io::AsRawFd;
use memmap2::MmapMut;

// Calculate buffer size
let stride = width * 4; // RGBA
let size = stride * height;

// Create memfd
let fd = memfd_create("wl_shm", MFD_CLOEXEC)?;
ftruncate(fd, size)?;

// mmap the memory
let mmap = unsafe {
    MmapMut::map_mut(&fd)?
};

// Create wl_shm_pool
let pool = shm.create_pool(fd.as_raw_fd(), size as i32, &qh, ());
let buffer = pool.create_buffer(
    0, width, height, stride,
    wl_shm::Format::Argb8888,
    &qh, ()
);
```

### 3. Request Screencopy

```rust
// Get first output (or specific one)
let output = /* wl_output from registry */;

// Request screencopy frame
let frame = screencopy_mgr.capture_output(
    overlay_cursor,  // 0 = no cursor, 1 = with cursor
    &output,
    &qh,
    ()
);
```

### 4. Handle Events

Implement Dispatch for screencopy frame events:

```rust
impl Dispatch<ZwlrScreencopyFrameV1, ()> for State {
    fn event(
        state: &mut Self,
        frame: &ZwlrScreencopyFrameV1,
        event: <ZwlrScreencopyFrameV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use zwlr_screencopy_frame_v1::Event;

        match event {
            Event::Buffer { format, width, height, stride } => {
                // Store buffer format info
                state.buffer_info = Some(BufferInfo {
                    format, width, height, stride
                });
            }
            Event::BufferDone => {
                // All buffer types enumerated, now copy
                frame.copy(&state.shm_buffer);
            }
            Event::Ready { .. } => {
                // Frame is ready in buffer!
                // Copy from mmap to our output buffer
                let pixels = state.mmap.as_slice();
                state.captured_frame = pixels.to_vec();
                state.frame_ready = true;
            }
            Event::Failed => {
                // Handle failure
                eprintln!("Screencopy failed");
            }
            _ => {}
        }
    }
}
```

### 5. Event Loop

```rust
// Dispatch events until frame is ready
while !state.frame_ready {
    event_queue.blocking_dispatch(&mut state)?;
}

// Now state.captured_frame contains the screen pixels
let frame_data = state.captured_frame;
```

---

## Integration with mm-warp

### Current Code (stub)

```rust
pub fn capture_frame(&mut self) -> Result<Vec<u8>> {
    tracing::warn!("capture_frame is stub - returns empty buffer");
    Ok(vec![0u8; 1920 * 1080 * 4])
}
```

### Replacement Pattern

```rust
pub struct WaylandConnection {
    connection: Connection,
    displays: Vec<Display>,
    screencopy_mgr: Option<ZwlrScreencopyManagerV1>,
    shm: Option<WlShm>,
    event_queue: EventQueue<CaptureState>,
}

pub fn capture_frame(&mut self) -> Result<Vec<u8>> {
    // 1. Get first output
    let output = self.get_first_output()?;

    // 2. Create shared memory buffer
    let (buffer, mmap) = self.create_shm_buffer(
        output.width, output.height
    )?;

    // 3. Request screencopy
    let frame = self.screencopy_mgr
        .capture_output(0, &output.wl_output, &qh, ());

    // 4. Event loop until ready
    let mut state = CaptureState::new(buffer, mmap);
    while !state.ready {
        self.event_queue.blocking_dispatch(&mut state)?;
    }

    // 5. Return captured pixels
    Ok(state.pixels)
}
```

---

## Dependencies Needed

Add to `Cargo.toml`:
```toml
[dependencies]
memmap2 = "0.9"  # For mmap
nix = { version = "0.29", features = ["fs"] }  # For memfd_create
```

Re-add to imports:
```rust
use wayland_client::protocol::{wl_shm, wl_output};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
    zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
};
use std::os::unix::io::AsRawFd;
```

---

## State Structure

```rust
struct CaptureState {
    buffer: WlBuffer,
    mmap: MmapMut,
    ready: bool,
    pixels: Vec<u8>,
    buffer_info: Option<BufferInfo>,
}

struct BufferInfo {
    format: u32,
    width: u32,
    height: u32,
    stride: u32,
}
```

---

## Implementation Steps

**Phase 1**: Basic capture (30min - 1hr)
- [ ] Add dependencies (memmap2, nix)
- [ ] Bind screencopy manager in registry
- [ ] Get first output with dimensions

**Phase 2**: Shared memory (1-2hrs)
- [ ] Create memfd
- [ ] Create wl_shm_pool
- [ ] Create wl_buffer

**Phase 3**: Capture (1-2hrs)
- [ ] Request screencopy frame
- [ ] Implement event handlers
- [ ] Handle buffer/buffer_done/ready events

**Phase 4**: Integration (1hr)
- [ ] Replace stub capture_frame
- [ ] Test with real screen
- [ ] Validate RGBA format

**Phase 5**: Polish (1hr)
- [ ] Error handling
- [ ] Multiple outputs support
- [ ] Cursor capture option

**Total**: ~6-8 hours spread over 1-2 days

---

## Testing Plan

**Step 1**: Verify capture works
```bash
cargo run --bin test_screencopy  # New binary to test capture
```

**Step 2**: Test with encoder
```bash
cargo run --bin server  # Should now encode real screen
cargo run --bin client  # Should see real desktop!
```

**Step 3**: Validate
- [ ] Can see actual desktop content
- [ ] Colors are correct
- [ ] Updates in real-time (30fps)
- [ ] No crashes/memory leaks

---

## References

**C Examples**:
- https://github.com/Ferdi265/wayland-experiments/blob/main/screencopy_shm.c

**Rust Tools** (for reference):
- wayshot: https://github.com/waycrate/wayshot
- haruhishot: https://github.com/Decodetalkers/haruhishot

**Protocol Docs**:
- https://docs.rs/wayland-protocols-wlr/
- https://wayland.app/protocols/wlr-screencopy-unstable-v1

---

## Challenges to Expect

**Event-driven async**: Wayland is inherently async (events arrive, must dispatch)
**Memory management**: Shared memory between compositor and client
**Format handling**: Multiple pixel formats possible (ARGB8888, XRGB8888, etc.)
**Multiple displays**: Need to query and select correct output

**But**: We have working examples to reference, and the protocol is well-documented.

---

🦬☀️ **This is the final piece. Everything else is complete and working.**
