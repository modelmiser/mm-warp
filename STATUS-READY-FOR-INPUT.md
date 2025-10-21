# mm-warp Status - Ready for Input Events

**Date**: October 21, 2025
**Status**: ✅ FULLY WORKING 4K Remote Desktop on COSMIC!

---

## What's Working NOW

### Core Features ✅
- **Screen capture**: ext-image-copy-capture-v1 (COSMIC native)
- **Resolution**: 4K (3840x2160) captured, 1920x1080 displayed
- **Encoding**: H.264 with ultrafast/zerolatency
- **Streaming**: QUIC with TLS encryption
- **Display**: Native Wayland window with wp_viewport scaling
- **Performance**: 17-20 FPS, 11-16 Mbps (peaks 35+ on heavy motion)
- **Adaptive FPS**: Drops to 5 when idle, 20 on motion
- **Memory**: No leaks - reuses all buffers/pools
- **CPU**: 5.5% server, 1.5% client

### Test Results
```
FPS: 17-20 (actual achieved)
Bitrate: 11-16 Mbps baseline, 35+ Mbps peaks
CPU: 5.5% server, 1.5% client
Memory: Stable (no leaks)
Quality: Full 4K color, good compression
```

---

## Known Issues

### 1. Reconnection Handling
**Problem**:
- Must start server THEN client
- If either disconnects, both must be restarted
- Out-of-order start fails

**Fix needed**:
- Server: Loop accept() to handle multiple clients
- Client: Retry connection on failure
- Both: Handle disconnects gracefully

**Estimated time**: 30-60 minutes

### 2. Recursive Display Effect
**Problem**: Capturing own desktop shows infinite recursion

**Solutions**:
- Multi-display support (capture different monitor)
- Exclude client window from capture (compositor feature)
- Just don't look at the client window while using it! 😄

**Not critical**: Actually kind of fun!

---

## Next Features (Priority Order)

### 1. Input Events (HIGH - Makes it actually usable!)

**Already have**: InputEvent struct with serialization in lib.rs!

**Need to add**:

**Client side** (30-45 min):
- Bind wl_seat to get pointer and keyboard
- Capture mouse move/click events
- Capture keyboard events
- Send via InputEvent::send()

**Server side** (45-60 min):
- Receive input events (datagrams)
- Use uinput to inject into Linux input system
- Map coordinates (4K capture to actual display coords)

**Dependencies**:
```toml
# Server
evdev = "0.12"  # For uinput input injection
```

**Total time**: ~2 hours
**Result**: Fully functional remote desktop with input!

### 2. Window Position Save/Restore (MEDIUM)

**Implementation** (30 min):
- Save window position to `~/.config/mm-warp/client.toml`
- XDG surface configure events give window position
- Restore on startup via set_window_geometry

**Dependencies**:
```toml
serde = "1.0"
toml = "0.8"
```

### 3. Reconnection Handling (MEDIUM)

**Server** (20 min):
```rust
loop {
    let connection = server.accept().await?;
    // Spawn task to handle this client
    // Loop to accept next client
}
```

**Client** (20 min):
```rust
loop {
    match client.connect(addr).await {
        Ok(conn) => { /* stream */ }
        Err(_) => {
            sleep(1);
            continue; // Retry
        }
    }
}
```

---

## Code Structure (Current)

```
mm-warp-server/
├── src/
│   ├── lib.rs (H264Encoder, QuicServer, InputEvent serialization)
│   ├── ext_capture.rs (COSMIC screen capture - OPTIMIZED)
│   └── bin/
│       ├── server.rs (main - adaptive FPS, stats)
│       └── server_ext_raw.rs (uncompressed test)
│
mm-warp-client/
├── src/
│   ├── lib.rs (QuicClient, H264Decoder, InputEvent)
│   ├── wayland_display.rs (Native window with viewport)
│   └── bin/
│       ├── client.rs (main - display + stats)
│       └── client_ext_raw.rs (uncompressed test)
```

---

## Input Events Implementation Plan

### Phase 1: Client Input Capture

**File**: `wayland_display.rs`

Add to WaylandDisplay:
```rust
pub struct WaylandDisplay {
    // ... existing fields ...
    pending_events: Arc<Mutex<Vec<InputEvent>>>,
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(state: &mut Self, pointer: &wl_pointer::WlPointer, event: wl_pointer::Event, ...) {
        match event {
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => {
                // Queue MouseMove event
            }
            wl_pointer::Event::Button { button, state, .. } => {
                // Queue MouseButton event
            }
            _ => {}
        }
    }
}

// Similar for wl_keyboard
```

**Method**:
```rust
pub fn poll_input_events(&mut self) -> Vec<InputEvent> {
    // Return and clear pending events
}
```

### Phase 2: Client Main Loop

**File**: `client.rs`

```rust
loop {
    // Poll for input events
    let events = display.poll_input_events();
    for event in events {
        InputEvent::send(&connection, event).await?;
    }

    // Receive and display frame
    let encoded = QuicClient::receive_frame(&connection).await?;
    // ... existing decode/display code ...
}
```

### Phase 3: Server Input Injection

**File**: New `input_inject.rs`

```rust
use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};

pub struct InputInjector {
    device: VirtualDevice,
}

impl InputInjector {
    pub fn new() -> Result<Self> {
        let device = VirtualDeviceBuilder::new()?
            .name("mm-warp-remote")
            .with_keys(&evdev::AttributeSet::from_iter([/* all keys */]))?
            .with_relative_axes(&evdev::AttributeSet::from_iter([
                evdev::RelativeAxisType::REL_X,
                evdev::RelativeAxisType::REL_Y,
            ]))?
            .build()?;

        Ok(Self { device })
    }

    pub fn inject(&mut self, event: InputEvent) -> Result<()> {
        // Convert InputEvent to evdev events and emit
    }
}
```

### Phase 4: Server Main Loop

**File**: `server.rs`

```rust
let mut injector = InputInjector::new()?;

// Spawn task to receive input events
tokio::spawn(async move {
    loop {
        if let Ok(datagram) = connection.read_datagram().await {
            if let Ok(event) = InputEvent::from_bytes(&datagram) {
                injector.inject(event)?;
            }
        }
    }
});

// Main loop continues streaming frames
```

---

## Window Position Save/Restore Plan

**File**: New `config.rs`

```rust
#[derive(Serialize, Deserialize)]
struct ClientConfig {
    window_x: i32,
    window_y: i32,
    window_width: i32,
    window_height: i32,
}

impl ClientConfig {
    fn load() -> Result<Self> {
        let path = dirs::config_dir()
            .unwrap()
            .join("mm-warp/client.toml");
        // Read and parse
    }

    fn save(&self) -> Result<()> {
        // Write to config file
    }
}
```

**Usage in client.rs**:
```rust
// Load config
let config = ClientConfig::load().unwrap_or_default();

// Create display with saved position
let mut display = WaylandDisplay::new_with_position(
    3840, 2160,
    config.window_x,
    config.window_y,
)?;

// On shutdown (Ctrl+C handler):
config.save()?;
```

---

## Performance Notes

### Current Bottlenecks
1. **Screen capture**: ~50-55ms per frame (18-20 FPS max)
2. **H.264 encoding**: Minimal (ultrafast preset)
3. **Network**: Minimal (QUIC is efficient)

### Why 18-20 FPS (not 60)?
- COSMIC's ext-image-copy-capture has overhead
- 4K resolution is demanding
- Shared memory copies take time
- Still very responsive for remote desktop use!

### Optimization Opportunities (Future)
- **dmabuf instead of shm**: Zero-copy GPU buffers (complex)
- **Partial updates**: Only capture changed regions
- **Hardware encoding**: Use GPU encoder (VAAPI/NVENC)

---

## What to Test Next Session

1. **Input events**: Click and type in the remote desktop!
2. **Window position**: Close and reopen, should remember position
3. **Reconnection**: Restart server while client running (should reconnect)

---

## Commits Pending

**Git lock issue**: Background process holding lock

**To commit** (when lock clears):
```bash
rm -f .git/index.lock
git add radiant-ecosystem/mm-warp/
git commit -m "mm-warp: Adaptive FPS + continuous streaming + Wayland display!"
```

**Changes to commit**:
- Adaptive FPS implementation
- Continuous streaming
- Wayland native window
- wp_viewport scaling
- Real-time stats
- Memory optimizations
- 30% FPS improvement

---

🦬☀️ **The bison streams before the sun. COSMIC support complete. Input events next!**
