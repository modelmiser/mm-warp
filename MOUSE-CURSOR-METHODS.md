# Mouse Cursor Injection Methods for mm-warp

**Date**: October 21, 2025
**Status**: Research complete, implementation deferred

---

## Current State

- ✅ **Cursor IS visible** in stream (Options::PaintCursors in ext-image-copy-capture)
- ❌ **Cursor movement** doesn't work via uinput
- ❌ **Mouse clicks** not implemented yet

---

## Why uinput Doesn't Work for Mouse on Wayland

uinput creates virtual input devices at kernel level (evdev):
- **On X11**: Applications read directly from evdev → uinput works
- **On Wayland**: Compositor manages ALL input
  - Compositor reads from real hardware devices only
  - uinput virtual devices are **ignored by compositor** (intentional security feature)
  - Compositor sends events to apps via Wayland protocols

**Result**: uinput keyboard works (compositor doesn't filter), but mouse movement is blocked.

---

## Available Methods

### Method 1: ydotool ⭐ **RECOMMENDED**

**How it works:**
- Standalone daemon (`ydotoold`) runs with root privileges
- Uses `/dev/uinput` at lower level that compositors respect
- Command-line tool (`ydotool`) sends events to daemon

**Pros:**
- ✅ Simple implementation (10 lines of code)
- ✅ Works on most Wayland compositors (COSMIC, Sway, Hyprland, GNOME, KDE)
- ✅ No protocol knowledge needed
- ✅ Production-ready NOW

**Cons:**
- ⚠️  External dependency (user must install)
- ⚠️  Requires ydotoold daemon running
- ⚠️  Command-line overhead (~1ms per event)

**Setup:**
```bash
# Install ydotool
sudo apt install ydotool  # Debian/Ubuntu
# OR build from source: https://github.com/ReimuNotMoe/ydotool

# Start daemon (needs root)
sudo ydotoold &

# Test
ydotool mousemove 500 500
ydotool click 0x40  # Left click
```

**Implementation (30 minutes):**
```rust
// Add to mm-warp-server/src/input_inject.rs

impl InputInjector {
    pub fn inject_mouse_move(&mut self, x: i32, y: i32) -> Result<()> {
        std::process::Command::new("ydotool")
            .args(&["mousemove", "--absolute", &x.to_string(), &y.to_string()])
            .output()
            .context("ydotool not found - install: sudo apt install ydotool")?;
        Ok(())
    }

    pub fn inject_mouse_button(&mut self, button: u32, pressed: bool) -> Result<()> {
        // Wayland button codes: 1=left, 2=middle, 3=right
        // ydotool codes: 0x40=left, 0x41=right, 0x42=middle
        let ydotool_button = match button {
            1 => "0x40", // BTN_LEFT
            2 => "0x42", // BTN_MIDDLE
            3 => "0x41", // BTN_RIGHT
            _ => return Ok(()), // Ignore unknown buttons
        };

        let action = if pressed { "click" } else { return Ok(()) };
        std::process::Command::new("ydotool")
            .args(&[action, ydotool_button])
            .output()
            .context("ydotool failed")?;
        Ok(())
    }
}
```

**Wire into server.rs:**
```rust
// In input receiver task
InputEvent::MouseMove { x, y } => {
    let _ = injector.inject_mouse_move(x, y);
}
InputEvent::MouseButton { button, pressed } => {
    let _ = injector.inject_mouse_button(button, pressed);
}
```

---

### Method 2: Wayland Protocols (Compositor-Specific)

**Critical Limitation**: Wayland has **NO standard input injection protocol** by design (security).

**Available compositor-specific protocols:**

#### wlroots-based (Sway, Hyprland, etc.)
- `wlr-virtual-pointer-unstable-v1` - Virtual pointer device
- `zwp-virtual-keyboard-v1` - Virtual keyboard (we use evdev instead)

**Implementation complexity:**
- 200+ lines of protocol binding code
- Compositor-specific (won't work on GNOME/KDE/COSMIC)
- Must maintain virtual device lifecycle
- Testing requires multiple compositors

**Example** (pseudocode):
```rust
// 1. Connect to Wayland display
// 2. Bind to wlr_virtual_pointer_manager_v1
// 3. Create virtual pointer device
// 4. Send motion events via protocol:
virtual_pointer.motion(time_ms, x, y);
virtual_pointer.button(time_ms, button_code, state);
virtual_pointer.frame(); // Commit changes
```

#### COSMIC (System76)
- **Status**: No known public input injection protocol
- COSMIC is new (2024), may add protocols in future
- Current assumption: Input injection not exposed (security by design)

**Recommendation**: Skip protocol approach until standard emerges.

---

### Method 3: KMS/DRM Direct (Not Recommended)

**How it works:**
- Bypass compositor entirely via kernel modesetting (KMS)
- Direct hardware control via DRM subsystem
- Requires `CAP_SYS_ADMIN` capability

**Pros:**
- ✅ Works regardless of compositor
- ✅ Ultimate control

**Cons:**
- ❌ Extremely complex (500+ lines)
- ❌ Breaks compositor assumptions (visual glitches)
- ❌ Security nightmare (full system access)
- ❌ Massive overkill for remote desktop

**Verdict**: Absolutely not worth it for mm-warp.

---

### Method 4: Accept Keyboard-Only (Pragmatic Fallback)

**Many power users navigate primarily with keyboard:**
- `Tab` / `Shift+Tab` - Switch focus
- `Arrow keys` - Navigate menus/lists
- `Enter` / `Space` - Activate/select
- `Alt+F4` - Close window
- `Super+arrows` - Tile windows
- Vim keybindings in terminals/editors

**Pros:**
- ✅ Already working (keyboard injection proven)
- ✅ Zero dependencies
- ✅ Lower latency (fewer events)
- ✅ Simpler codebase

**Cons:**
- ⚠️  Some GUI apps require mouse (CAD, image editing, gaming)
- ⚠️  Less intuitive for non-technical users

**Use case**: Server administration, terminal work, text editing (90% of remote desktop use).

---

## Recommendation: ydotool for v1.0

**Decision Matrix:**

| Method | Complexity | Works Now | Portable | Maintenance |
|--------|-----------|-----------|----------|-------------|
| **ydotool** | ⭐ Low | ✅ Yes | ✅ Most compositors | ✅ Minimal |
| Wayland protocols | ⚠️ High | ⚠️ Partial | ❌ wlroots only | ⚠️ Per-compositor |
| KMS/DRM | ❌ Very High | ⚠️ Yes | ✅ All | ❌ High risk |
| Keyboard-only | ⭐ None | ✅ Yes | ✅ All | ✅ None |

**Chosen: ydotool** because:
1. **30-minute implementation** vs 4+ hours for protocols
2. **Works today** on COSMIC, Sway, Hyprland, GNOME, KDE
3. **Acceptable dependency** - users expect to install tools for remote desktop (like VNC, xrdp)
4. **Future-proof** - If Wayland standardizes input injection, we can migrate

**Fallback**: Document keyboard-only navigation for users without ydotool.

---

## Implementation Plan (v1.0 - Next Session)

**Time estimate**: 1 hour

**Tasks:**
1. Add `inject_mouse_move()` to `input_inject.rs` (15 min)
2. Add `inject_mouse_button()` to `input_inject.rs` (15 min)
3. Wire into server.rs input receiver (5 min)
4. Test with ydotool installed (10 min)
5. Update README with ydotool setup instructions (15 min)

**README addition:**
```markdown
### Mouse Control (Optional)

mm-warp supports mouse control via ydotool.

**Setup:**
```bash
# 1. Install ydotool
sudo apt install ydotool

# 2. Start daemon
sudo ydotoold &

# 3. Run mm-warp server as usual
sudo ./target/release/server
```

**Without ydotool**: Keyboard control still works. Use Tab/Arrow keys to navigate.
```

---

## Future (v2.0+)

**Monitor Wayland protocol development:**
- Watch for standardized input injection protocol
- Track COSMIC-specific protocols as they emerge
- Consider migrating if standard emerges

**But don't wait** - ydotool is production-ready NOW.

---

## Technical Details

### ydotool Button Codes

| Mouse Button | Wayland Code | ydotool Hex |
|-------------|--------------|-------------|
| Left        | 1 (BTN_LEFT) | 0x40        |
| Middle      | 2 (BTN_MIDDLE) | 0x42      |
| Right       | 3 (BTN_RIGHT) | 0x41       |

### Performance

- **ydotool overhead**: ~1ms per command (process spawn)
- **Acceptable**: Mouse events are ~60-100 Hz max (10-16ms between events)
- **Optimization**: Could use ydotool's daemon socket directly (avoid spawn overhead)

### Security Model

**Why Wayland blocks uinput:**
- Prevents keyloggers from injecting fake input
- Stops background apps from controlling cursor
- Requires explicit user action (running ydotoold daemon)

**mm-warp security:**
- Server requires sudo (uinput access)
- ydotoold requires sudo (same reason)
- User explicitly grants permission by running daemon
- No different from VNC/RDP security model

---

## References

- [ydotool GitHub](https://github.com/ReimuNotMoe/ydotool)
- [Wayland security model](https://wayland.freedesktop.org/architecture.html)
- [wlr-protocols](https://gitlab.freedesktop.org/wlroots/wlr-protocols)
- evdev documentation: `/usr/include/linux/input-event-codes.h`

---

🦬☀️ **Pragmatic solution over perfect solution. Ship ydotool support now, iterate later.**
