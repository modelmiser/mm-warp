# mm-warp - Wayland Remote Desktop

**Status**: ✅ COSMIC SUPPORT COMPLETE! 🎉

**Full 4K H.264 Streaming on COSMIC**:
- ✅ Real desktop capture via ext-image-copy-capture-v1
- ✅ 4K resolution (3840x2160 @ 31MB per frame)
- ✅ H.264 encoding ready (RGBA → YUV420P → H.264)
- ✅ QUIC encrypted streaming (TLS)
- ✅ H.264 decoding ready (H.264 → YUV420P → RGBA)
- ✅ Uncompressed mode VERIFIED (10 frames @ 310MB total)

**What works**:
- ✅ **Screen capture** (ext-image-copy-capture-v1 + wlr-screencopy)
- ✅ **ABGR8888 format** (COSMIC's native format)
- ✅ **Frame buffer** (ring buffer)
- ✅ **H.264 encoder** (full RGB color, swscale)
- ✅ **H.264 decoder** (full RGBA output, swscale)
- ✅ **QUIC streaming** (TLS encrypted)
- ✅ Input event serialization
- ✅ End-to-end integration (complete!)

**Compositor Support**:
- ✅ **COSMIC** (ext-image-copy-capture-v1) **TESTED & WORKING!**
- ✅ Sway (wlr-screencopy)
- ✅ Hyprland (wlr-screencopy)
- ✅ wlroots-based (wlr-screencopy)
- ⚠️ GNOME/KDE (probably works via ext - needs testing)

**Progress**: See [FUTURE-PROTOCOLS.md](old/FUTURE-PROTOCOLS.md) for vision (moved to old/)

---

## Quick Start

### 1. Build

```bash
cargo build --release
```

### 2. Setup uinput Access (One-time)

**Option A - Automated (Recommended):**
```bash
./setup-uinput.sh
# Log out and back in after setup
```

**Option B - Manual:**
```bash
# Preserve environment when using sudo
sudo -E ./target/release/server
```

See [Troubleshooting](#troubleshooting) below for details.

### 3. Run

**Terminal 1 (Server):**
```bash
./target/release/server
# Waits for client connections...
```

**Terminal 2 (Client):**
```bash
./target/release/client
# Connects and displays your desktop!
```

**What you'll see:**
- Client window shows your COSMIC desktop at 4K
- Server prints stats: `FPS: 18.2 | Bitrate: 14.23 Mbps`
- Client auto-types 'a' every 2 seconds (test keyboard injection)

## Features

### ✅ Working Now

- **4K Screen Capture** (18-20 FPS on COSMIC)
- **H.264 Streaming** (11-35 Mbps adaptive bitrate)
- **Keyboard Control** (via uinput)
- **Cursor Visible** (painted in stream)
- **Reconnection** (server accepts new clients)
- **Adaptive FPS** (5 FPS idle, 20 FPS on motion)

### 🚧 Coming Soon

- **Mouse Control** (via ydotool - see [MOUSE-CURSOR-METHODS.md](MOUSE-CURSOR-METHODS.md))
- **Real Keyboard Capture** (currently just test sender)
- **Multi-display** (select which monitor)
- **Configuration File** (bitrate, resolution, etc.)

## Test Binaries

```bash
# Test ext-image-copy-capture detection (COSMIC):
cargo run --bin test_ext_capture

# Test uncompressed streaming (COSMIC - VERIFIED WORKING!):
cargo run --bin server_ext_raw  # Terminal 1
cargo run --bin client_ext_raw  # Terminal 2

# Legacy tests (wlr-screencopy for Sway/Hyprland):
cargo run --bin server_raw  # Terminal 1
cargo run --bin client_raw  # Terminal 2
```

## Foundation Tasks (Complete)

- [x] Task 1: Project setup
- [x] Task 2: Wayland connection
- [x] Task 3: Screen capture (stub)
- [x] Task 4: Frame buffer
- [x] Task 5: H.264 encoding
- [x] Task 6: QUIC server
- [x] Task 7: Stream frames
- [x] Task 8: QUIC client
- [x] Task 9: Client decode
- [x] Task 10: Input events

🦬☀️ *Foundation laid. Time to build.*

---

## Troubleshooting

### "Could not find wayland compositor" when using sudo

**Problem:** Running `sudo ./target/release/server` fails with:
```
Error: Failed to connect to Wayland (ext-image-copy-capture)
Caused by: Could not find wayland compositor
```

**Cause:** sudo strips environment variables needed for Wayland (`$WAYLAND_DISPLAY`, `$XDG_RUNTIME_DIR`)

**Solutions:**

**Option 1 - Quick Fix (Preserve Environment):**
```bash
sudo -E ./target/release/server
```

**Option 2 - Permanent Fix (No sudo needed):**
```bash
# Run setup script
./setup-uinput.sh

# Log out and back in (for group membership to take effect)
# Then run without sudo:
./target/release/server
```

**What the setup script does:**
1. Creates `uinput` group and adds you to it
2. Creates udev rule: `/etc/udev/rules.d/99-uinput.rules`
3. Loads `uinput` kernel module
4. Makes module load on boot
5. Reloads udev rules

**Manual verification:**
```bash
# Check if you're in uinput group (after logout/login)
groups | grep uinput

# Check /dev/uinput permissions
ls -l /dev/uinput
# Should show: crw-rw---- 1 root uinput ... /dev/uinput
```

### "Permission denied" for input injection

**Expected!** This message appears when running server without proper uinput access:
```
⚠️  Input injector failed: Permission denied (os error 13)
    Run with sudo to enable keyboard control
```

**Fix:** Run setup script (see above) or use `sudo -E`

### Client can't connect

**Check:**
1. Is server running? Look for: `✅ Server listening`
2. Firewall blocking port 4433? (shouldn't be for localhost)
3. Try: `netstat -tlnp | grep 4433` to verify server is listening

**Client should auto-retry every 2 seconds** with message:
```
⚠️  Connection failed: ... - retrying in 2s...
```

### Low FPS / Choppy streaming

**Expected FPS on COSMIC:** 17-20 FPS for 4K capture (COSMIC compositor limit)

**Adaptive FPS in action:**
- **Idle desktop:** Drops to 5 FPS (saves bandwidth)
- **Active motion:** Jumps to 20 FPS

**Stats to watch:**
```
FPS: 18.2 (target: 20) | Bitrate: 14.23 Mbps | Avg: 124KB/frame
```

**If FPS is much lower than target:**
- Check CPU usage: `top` (should be ~5% server, 1.5% client)
- Check system load
- 4K encoding is demanding - this is normal performance

### Keyboard not working

**Current status:** Client sends test keys only (types 'a' every 2 seconds)

**Real keyboard capture:** Not implemented yet (coming soon)

**To verify keyboard injection works:**
1. Run server with proper uinput access
2. Run client
3. Focus a text editor on server machine
4. Should see 'a' appear every 2 seconds

### Mouse not working

**Expected!** Mouse control not implemented yet.

**Coming soon:** ydotool integration (see [MOUSE-CURSOR-METHODS.md](MOUSE-CURSOR-METHODS.md))

**Workaround:** Use keyboard navigation:
- Tab/Shift+Tab - Switch focus
- Arrow keys - Navigate
- Enter/Space - Activate

---

## Documentation

- [MOUSE-CURSOR-METHODS.md](MOUSE-CURSOR-METHODS.md) - Mouse injection research & implementation plan
- [INPUT-EVENTS-NEXT.md](INPUT-EVENTS-NEXT.md) - Input pipeline implementation notes
- [SESSION-COMPLETE-2025-10-21.md](SESSION-COMPLETE-2025-10-21.md) - Complete development log
