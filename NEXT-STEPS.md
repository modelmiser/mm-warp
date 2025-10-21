# mm-warp - Next Steps

**Current Status**: H.264 streaming pipeline WORKING ✅

**Last Session**: 2025-10-21 - Fixed H.264 encoder, validated end-to-end streaming

---

## What Works Now

✅ **Full Pipeline**:
- Server encodes 1920×1080 frames to H.264 (grayscale)
- QUIC streams encrypted video over network
- Client decodes H.264 back to RGBA
- 30x compression (307KB → 10KB per frame)
- Zero latency mode (immediate encoding)

✅ **Both Modes**:
- Uncompressed: 320×240 RGBA streaming (`server_raw`/`client_raw`)
- H.264 compressed: 1920×1080 streaming (`server`/`client`)

✅ **Test Suite**:
- Integration tests pass
- Debug tools for encoder troubleshooting
- Live demo validated (10 frames successfully streamed)

---

## Next Priorities

### 1. Real Wayland Screen Capture ✅ **COMPLETE**

**Status**: wlr-screencopy-unstable-v1 **fully implemented!**

**What Works**:
- ✅ Binds to `zwlr_screencopy_manager_v1`
- ✅ Creates shared memory pool (`wl_shm` + memfd)
- ✅ Requests screencopy to shm buffer
- ✅ Handles frame ready events (async)
- ✅ Copies buffer data (ARGB→RGBA conversion)

**Compositor Support**:
- ✅ **Sway**: Works
- ✅ **Hyprland**: Works
- ✅ **wlroots-based**: Works
- ❌ **COSMIC**: Needs ext-image-copy-capture-v1 (see below)
- ❌ **GNOME/KDE**: Needs portal or different protocol (see below)

**Test**: `cargo run --bin test_screencopy` (requires wlroots compositor)

---

### 1a. COSMIC Compositor Support (NEW - FUTURE WORK)

**Current**: COSMIC uses ext-image-copy-capture-v1, not wlr-screencopy
**Impact**: mm-warp doesn't work on COSMIC desktop

**Option A - Direct Protocol** (2-3 hours):
- [ ] Download ext-image-copy-capture-v1.xml from wayland-protocols
- [ ] Generate Rust bindings using wayland-scanner
- [ ] Implement capture (similar pattern to wlr-screencopy)
- [ ] Add protocol detection + fallback logic
- [ ] Test on COSMIC

**Option B - Universal Portal** (4-6 hours):
- [ ] Complete PipeWire integration (complex POD API)
- [ ] Use XDG Desktop Portal for screen capture
- [ ] Works on ALL compositors (COSMIC, GNOME, KDE, Sway)
- [ ] Trade-off: User permission prompts, higher latency

**Recommendation**: Start with Option A (COSMIC-specific), add Option B later if needed.

---

### 2. Full RGB Color Support (MEDIUM PRIORITY)

**Current**: Grayscale YUV (U/V planes = 128)
**Need**: Proper RGB→YUV420P conversion

**Option A - Quick (swscale)**:
```rust
use ffmpeg_next::software::scaling::{context::Context, flag::Flags};

// Convert RGBA → YUV420P using swscale
let mut scaler = Context::get(
    format::Pixel::RGBA,
    width, height,
    format::Pixel::YUV420P,
    width, height,
    Flags::BILINEAR,
)?;

scaler.run(&rgba_frame, &mut yuv_frame)?;
```

**Option B - Manual (learning)**:
- Implement RGB→YUV conversion formulas
- Better understanding, more control
- Slightly more code

**Complexity**: Low
**Timeline**: 2-4 hours

---

### 3. Decoder Output Format (LOW PRIORITY)

**Current**: Decoder returns stub RGBA buffer
**Need**: Convert decoded YUV420P → RGBA for display

**Tasks**:
- [ ] Use swscale to convert YUV420P → RGBA
- [ ] Return properly formatted frame
- [ ] Validate colors match input

**Complexity**: Low (mirror of encoder conversion)
**Timeline**: 1-2 hours

**Note**: Not blocking - decoder successfully decodes, just needs output conversion

---

### 4. Performance & Polish (FUTURE)

**Once core features work**:
- [ ] Profile encoding performance
- [ ] Optimize frame capture rate
- [ ] Add configurable bitrate/quality
- [ ] Implement adaptive bitrate (based on network)
- [ ] Add latency measurements
- [ ] Proper error recovery

---

## Recommended Sequence

**Week 1** (Prove it works with real screen):
1. Implement Wayland screencopy (1-2 days)
2. Add RGB color conversion (2-4 hours)
3. Test with real desktop streaming
4. **Milestone**: Stream actual desktop @ 30fps

**Week 2** (Polish):
1. Fix decoder output format
2. Performance optimization
3. Add configurability
4. **Milestone**: Usable for real work

**Week 3** (Nice-to-haves):
1. Input event handling (mouse/keyboard)
2. Multiple display support
3. Audio streaming (future)
4. Session management

---

## Technical Debt to Address

**Warnings** (non-critical):
```
unused imports: wl_output, wl_shm (will use for screencopy)
unused imports: zwlr_screencopy_* (will use for screencopy)
unused variable: mut server_config (cosmetic)
```

**Fix**: `cargo fix --lib -p mm-warp-server`

---

## Success Criteria

**MVP Complete** when:
- ✅ H.264 streaming works (DONE)
- ⬜ Real Wayland screencopy (captures actual screen)
- ⬜ Full RGB color (not grayscale)
- ⬜ 30fps sustained streaming
- ⬜ Input events work (can control remote desktop)

**Production Ready** when:
- ⬜ Error recovery works
- ⬜ Network resilience (handles packet loss)
- ⬜ Multiple displays supported
- ⬜ Configuration system
- ⬜ Proper logging/debugging

---

## Notes

**Architecture is solid**:
- QUIC layer works perfectly
- H.264 encoding solved (zerolatency config)
- Client/server communication stable

**Biggest win**: Systematic debugging approach found encoder issue. Debug tools (`debug_encoder*.rs`) show methodology for future problems.

**Philosophy**: Build incrementally, prove each piece, iterate. The 🦬☀️ way.

---

🦬☀️ **The foundation is laid. Time to build the real thing.**
