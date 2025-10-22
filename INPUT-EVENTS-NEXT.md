# Input Events - Next Steps

**Status**: Keyboard injection proven working, needs pipeline integration

---

## What Works Now

**Keyboard injection tested** ✅:
```bash
sudo ./target/release/test_uinput
```
- Types "test" into focused window
- uinput device creation works
- Event synthesis with SYN_REPORT works

**What's missing**:
- Capturing keyboard events on client window
- Sending events from client to server
- Receiving events on server
- Injecting received events

---

## Implementation Plan (1-2 hours)

### Step 1: Server - Add Input Receiver (30 min)

**File**: `server.rs`

```rust
// After accepting connection, spawn input handler
let connection_clone = connection.clone();
tokio::spawn(async move {
    let mut injector = InputInjector::new().expect("Failed to create injector");
    loop {
        match connection_clone.read_datagram().await {
            Ok(bytes) => {
                if let Ok(event) = InputEvent::from_bytes(&bytes) {
                    match event {
                        InputEvent::KeyPress { key } => {
                            let _ = injector.inject_key(key, true);
                        }
                        InputEvent::KeyRelease { key } => {
                            let _ = injector.inject_key(key, false);
                        }
                        _ => {} // Mouse events ignored for now
                    }
                }
            }
            Err(_) => break,
        }
    }
});
```

**Need**: InputEvent::from_bytes() in client lib (add to lib.rs)

### Step 2: Client - Simple Keyboard Test (30 min)

**Simplest approach**: Don't capture from window yet, just send test keys

**File**: `client.rs`

```rust
// After display creation, spawn keyboard sender
let connection_clone = connection.clone();
tokio::spawn(async move {
    // Simple test: send 'a' key every 2 seconds
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let _ = InputEvent::send(&connection_clone, InputEvent::KeyPress { key: 30 }).await; // 'a'
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let _ = InputEvent::send(&connection_clone, InputEvent::KeyRelease { key: 30 }).await;
    }
});
```

**Test**: Focus text editor on server, should see 'a' typed every 2 seconds!

### Step 3: Client - Real Keyboard Capture (1-2 hours)

**Challenge**: Wayland keyboard capture requires proper event queue management

**Approaches**:

**Option A - Manual event dispatch**:
Create event queue in client.rs main loop, dispatch events each frame

**Option B - Use existing libraries**:
Check if winit or smithay-client-toolkit can help

**Option C - Simple SDL2**:
Use SDL2 window instead of raw Wayland (gets input for free)

---

## Mouse Cursor Options

### Current State
- ✅ Cursor **visible** (Options::PaintCursors)
- ❌ Cursor **movement** doesn't work (uinput limitation)

### Solutions

**Option 1: ydotool** (external tool):
```bash
# Install ydotool
sudo apt install ydotool

# Use in input_inject.rs
std::process::Command::new("ydotool")
    .args(&["mousemove", &x.to_string(), &y.to_string()])
    .spawn()?;
```

**Option 2: COSMIC input protocol**:
Research if COSMIC has specific input injection protocol

**Option 3: Accept keyboard-only for v1**:
Many users navigate with keyboard anyway (Tab, arrow keys, etc.)

---

## Recommended Next Session Plan

**Goal**: Get keyboard control working end-to-end

**Tasks**:
1. Add InputEvent::from_bytes() to client lib (5 min)
2. Add input receiver on server (10 min)
3. Add simple test keyboard sender on client (10 min)
4. Test: Server types 'a' every 2 seconds when server running (5 min)
5. If working, add real Wayland keyboard capture (1 hour)

**Total**: ~1.5-2 hours for full keyboard control

**Mouse**: Research ydotool or accept keyboard-only for v1

---

🦬☀️ **Cursor visible. Keyboard injection proven. Pipeline next!**
