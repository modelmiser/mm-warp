# mm-warp 🌀

**Native Wayland Remote Desktop for COSMIC (and other compositors)**

A modern remote desktop solution built on Wayland protocols, QUIC networking, and H.264 streaming. Designed for COSMIC, works on Sway/Hyprland/wlroots compositors.

## Status: Working Prototype (Alpha)

**Working features:**
- 🖥️ **4K screen capture** at 18-20 FPS on COSMIC
- 🎬 **H.264 streaming** with adaptive bitrate (11-35 Mbps)
- ⌨️ **Full keyboard control** (Wayland capture + uinput injection)
- 🖱️ **Mouse control** (pure evdev via uinput — no external tools needed)
- 🔒 **QUIC encryption** (TLS-encrypted; note: certificate verification is disabled for self-signed dev certs)
- 🔄 **Robust reconnection** (server runs continuously)
- 📊 **Real-time stats** (FPS, bitrate, frame size)

**Tested on:** COSMIC (System76 Pop!_OS)
**Also supports:** Sway, Hyprland, wlroots-based compositors

## Why mm-warp?

**Existing solutions have issues:**
- VNC/RDP: X11-era protocols, poor Wayland support
- Commercial tools: Closed source, vendor lock-in
- Screencast tools: View-only, no input control

**mm-warp is:**
- ✅ Native Wayland (uses compositor protocols directly)
- ✅ Modern stack (Rust, QUIC, H.264)
- ✅ Open source (GPL-3 license - COSMIC ecosystem compatible)
- ✅ Honest about limitations (see [RemoteDesktop portal notes](#documentation))

**Note:** Input injection uses uinput (kernel-level virtual devices) because COSMIC hasn't implemented the RemoteDesktop portal yet. See [pop-os/xdg-desktop-portal-cosmic#23](https://github.com/pop-os/xdg-desktop-portal-cosmic/issues/23). No external tools are required.

---

## Quick Start

### 1. Install Build Dependencies

**Debian/Ubuntu/Pop!_OS:**
```bash
sudo apt install libavcodec-dev libavformat-dev libswscale-dev libavutil-dev \
                 libwayland-dev pkg-config clang
```

**Fedora:**
```bash
sudo dnf install ffmpeg-devel wayland-devel clang pkg-config
```

**Arch:**
```bash
sudo pacman -S ffmpeg wayland clang pkg-config
```

### 2. Build

```bash
cargo build --release
```

### 3. Setup uinput Access (One-time)

**Option A - Automated (Recommended):**
```bash
./setup-uinput.sh
# Log out and back in after setup
```

**Option B - Manual:**
```bash
# Preserve environment when using sudo
sudo -E ./target/release/mm-warp-server
```

See [Troubleshooting](#troubleshooting) below for details.

### 4. Run

**Terminal 1 (Server):**
```bash
# Local testing (localhost only):
./target/release/mm-warp-server

# Remote desktop (listen on all interfaces — ⚠️ no authentication yet):
./target/release/mm-warp-server --listen 0.0.0.0:4433
```

**Terminal 2 (Client):**
```bash
# Connect (--insecure required until TOFU cert pinning is implemented):
./target/release/mm-warp-client --insecure

# Connect to remote server:
./target/release/mm-warp-client --insecure --server 192.168.1.100:4433
```

The client auto-detects resolution from the server — no `--resolution` flag needed.

**What you'll see:**
- Client window shows your COSMIC desktop at 4K
- Server prints stats: `[SERVER] FPS: 18.2 (limit: 60) | Bitrate: 14.23 Mbps`
- Client prints stats: `[CLIENT] FPS: 18.0 | Bitrate: 14.21 Mbps`

**For remote control:**
- **Focus the client window and type** - keystrokes appear on server machine
- **Move mouse in client window** - cursor moves on server machine

**IMPORTANT: How Input Injection Works**

**✅ For actual remote desktop (different machines):**
- Client machine → Type in client window
- Server machine → Input appears in focused applications
- **This works perfectly!** (Tested and verified)

**⚠️ For local testing (same machine):**
- Input injection is global (uinput injects system-wide)
- Goes to whatever window is focused (not specific to captured desktop)
- Confusing for testing, but correct behavior for remote access

**Why it's this way:**
- uinput injects at kernel level (global, not per-session)
- Wayland compositors don't support session-bound input injection yet
- This is a Wayland ecosystem limitation, not mm-warp bug
- Proper solution: RemoteDesktop portal (COSMIC hasn't implemented it yet — see [pop-os/xdg-desktop-portal-cosmic#23](https://github.com/pop-os/xdg-desktop-portal-cosmic/issues/23))

## Features

### ✅ Working Now

- **4K Screen Capture** (18-20 FPS on COSMIC)
- **H.264 Streaming** (11-35 Mbps adaptive bitrate)
- **Full Keyboard Control** (real Wayland keyboard capture)
- **Full Mouse Control** (movement + clicks via uinput)
- **Cursor Visible** (painted in stream)
- **Reconnection** (server accepts new clients)
- **Adaptive FPS** (5 FPS idle, 20 FPS on motion)

### 🚧 Future Enhancements

- **Multi-display** (select which monitor)
- **Configuration File** (bitrate, resolution, etc.)
- **Audio Streaming** (synchronized with video)

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

**Problem:** Running `sudo ./target/release/mm-warp-server` fails with:
```
Error: Failed to connect to Wayland (ext-image-copy-capture)
Caused by: Could not find wayland compositor
```

**Cause:** sudo strips environment variables needed for Wayland (`$WAYLAND_DISPLAY`, `$XDG_RUNTIME_DIR`)

**Solutions:**

**Option 1 - Quick Fix (Preserve Environment):**
```bash
sudo -E ./target/release/mm-warp-server
```

**Option 2 - Permanent Fix (No sudo needed):**
```bash
# Run setup script
./setup-uinput.sh

# Log out and back in (for group membership to take effect)
# Then run without sudo:
./target/release/mm-warp-server
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
3. Try: `ss -ulnp | grep 4433` to verify server is listening (QUIC uses UDP)

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

**Check:**
1. Is server running with proper uinput access?
2. Is the client window **focused**? (Wayland only captures input from focused window)
3. Try typing in a text editor on the server

**Still not working?** Check server output for input injector errors.

### Mouse not working

**Check uinput access:**
```bash
# Check if you're in uinput group (after logout/login)
groups | grep uinput

# Check /dev/uinput permissions
ls -l /dev/uinput
# Should show: crw-rw---- 1 root uinput ... /dev/uinput
```

**If permission denied:** Run `./setup-uinput.sh` or use `sudo -E` (see [Setup uinput Access](#2-setup-uinput-access-one-time) above).

---

## Documentation

- **RemoteDesktop portal**: COSMIC hasn't implemented the RemoteDesktop portal yet ([pop-os/xdg-desktop-portal-cosmic#23](https://github.com/pop-os/xdg-desktop-portal-cosmic/issues/23)). Input injection uses uinput (kernel-level virtual devices) as a workaround. No external tools are needed.
- **Input injection**: Both keyboard and mouse use pure evdev via uinput. See [Setup uinput Access](#2-setup-uinput-access-one-time) for permissions setup.
- **Encryption caveat**: QUIC transport is TLS-encrypted, but certificate verification is currently disabled (`SkipVerification` in client) to accept the server's self-signed cert. This means the connection is encrypted but not authenticated — a production deployment should use proper certificate verification or a trust-on-first-use scheme.
