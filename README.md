# mm-warp - Wayland Remote Desktop

**Status**: Foundation Complete ✅ (10/10 tasks)

**What works**:
- ✅ Wayland connection (enumerate displays)
- ✅ Frame buffer (ring buffer for frames)
- ✅ H.264 encoder (YUV420P, proper color space)
- ✅ H.264 decoder (ffmpeg-based)
- ✅ QUIC server (self-signed certs)
- ✅ QUIC client (cert verification skip for dev)
- ✅ Input event serialization (keyboard/mouse)
- ✅ Test binaries (test_encode, test_decode)

**Next**: Integration testing, then real Wayland screencopy

**Progress**: See [FUTURE-PROTOCOLS.md](old/FUTURE-PROTOCOLS.md) for vision (moved to old/)

---

## Build & Test

```bash
cargo build
cargo test

# Test encoder
cargo run --bin test_encode

# Test decoder
cargo run --bin test_decode
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
