# Cold Review Prompt — mm-warp v1

## PROJECT SUMMARY

mm-warp is a Wayland remote desktop system: H.264 over QUIC with input forwarding.
3 crates: mm-warp-common (shared types), mm-warp-server (capture + encode + serve),
mm-warp-client (receive + decode + display + input).

Key subsystems:
- Screen capture: ext-image-copy-capture-v1 (preferred) and wlr-screencopy (fallback)
- Video: H.264 encode (server) / decode (client) via ffmpeg-next
- Transport: QUIC via quinn, TLS with TOFU cert pinning or --insecure skip
- Input: uinput injection (server), Wayland keyboard/pointer events (client)
- Display: double-buffered Wayland shm surface with wp_viewporter scaling
- Protocol: metadata stream (13 bytes), per-frame uni streams (4-byte len + data), datagrams (input)
- Auth: optional PIN over bidi QUIC stream
- Adaptive FPS: server throttles based on frame size (idle detection)

Architecture: pipelined — capture (main thread, !Send) → encode (blocking thread) → send (async task).
Channels: mpsc(2) between stages. watch channel for FPS feedback from send → capture.

## FILE ASSIGNMENT

### Agent 1: COMMON + SERVER-LIB (core protocol + encoding + capture)
- `mm-warp-common/src/lib.rs` (185 lines — StreamMetadata, Resolution, config_dir, cert_fingerprint, wayland_dispatch_noop macro)
- `mm-warp-common/src/input_event.rs` (315 lines — InputEvent serialization)
- `mm-warp-common/src/pixel.rs` (112 lines — ARGB↔RGBA conversion)
- `mm-warp-common/src/buffer.rs` (27 lines — memfd helper)
- `mm-warp-common/src/stats.rs` (112 lines — StreamStats)
- `mm-warp-server/src/lib.rs` (588 lines — H264Encoder, QuicServer, WaylandConnection/screencopy, unsafe Send impl)
- `mm-warp-server/src/capture.rs` (12 lines — FrameSource trait)

**Complexity note:** server/lib.rs has `unsafe impl Send` and raw ffmpeg FFI calls. Review these carefully.

### Agent 2: SERVER BINARIES + INPUT (application logic + injection)
- `mm-warp-server/src/bin/server.rs` (390 lines — pipelined main loop, PIN auth, adaptive FPS, input receiver task)
- `mm-warp-server/src/input_inject.rs` (242 lines — uinput keyboard/mouse, key allowlist, coordinate normalization)
- `mm-warp-server/src/ext_capture.rs` (325 lines — ext-image-copy-capture-v1, Wayland dispatch, pixel format comment)
- `mm-warp-server/src/bin/debug_encoder.rs` (114 lines)
- `mm-warp-server/src/bin/test_pixel_format.rs` (125 lines)
- `mm-warp-server/tests/integration_test.rs` (83 lines)

### Agent 3: CLIENT (QUIC client, TLS/TOFU, decoder, display, input forwarding)
- `mm-warp-client/src/lib.rs` (434 lines — QuicClient, H264Decoder, TofuVerifier, SkipVerification)
- `mm-warp-client/src/bin/client.rs` (161 lines — reconnect loop, PIN auth client side, session)
- `mm-warp-client/src/wayland_display.rs` (301 lines — double-buffered display, input event dispatch)
- `mm-warp-client/src/bin/client_raw.rs` (47 lines)
- `mm-warp-client/src/bin/client_ext_raw.rs` (48 lines)
- `mm-warp-client/src/bin/test_decode.rs` (27 lines)
- `mm-warp-client/tests/integration_test.rs` (58 lines)

## SEVERITY

- **CRITICAL**: Memory safety, undefined behavior, data races, use-after-free, integer overflow leading to buffer overrun, security bypass. Also: `unsafe` blocks with wrong invariant assumptions.
- **MODERATE**: Logic errors that produce wrong behavior (dropped frames, stuck keys, wrong pixel data), protocol mismatches between client/server, resource leaks, incorrect error handling that masks failures, missing bounds checks on untrusted network input.
- **LOW**: Style, naming, dead code, minor doc inaccuracy, non-idiomatic patterns, performance opportunities.

## KNOWN PATTERNS

*R1 fixes (5 MODERATE):*
- StreamMetadata::from_bytes() validates dimensions: rejects 0 and >16384 (common/lib.rs)
- Server PIN rejection closes QUIC connection with reason code (server.rs)
- Server PIN read uses read_to_end(256) for complete stream read (server.rs)
- Client PIN exchange has 10s timeout (client.rs)
- Stale doc comment removed (client/lib.rs)

## KNOWN UNTESTED

*Design-level (deferred from R1):*
- WaylandConnection::resolution() returns 1920x1080 default before first capture — encoder mismatch on non-1080p wlr-screencopy displays [ARCHITECTURAL]
- No PIN brute-force protection — rate limiting/lockout not implemented [FEATURE]
- Rate limiter permits 2000-event bursts at window boundaries [ACCEPTABLE-TRADEOFF]
- H264Decoder resolution change not propagated to display — latent, server sends fixed [LATENT]
- u32 overflow in stride/size calculations for extreme resolutions — unreachable via 16384 limit [THEORETICAL]
- Mouse coordinate rounding error for odd capture widths [COSMETIC]
- ext_capture frame objects not explicitly destroyed — wayland-client may handle via Drop [UNCLEAR]
- client_ext_raw expects uncompressed 4K (33MB) but MAX_FRAME_SIZE=5MB — test binary broken [TEST-ONLY]

*LOWs (accepted):*
- pixel.rs: debug_assert only — release builds get index-out-of-bounds panic instead of clear error
- wayland_display.rs: dispatch_pending error silently ignored (subsequent flush catches it)
- input_inject.rs: scroll value unbounded from network (compositor handles extreme values)
- server/lib.rs: hardcoded Argb8888 format ignores compositor's reported format (alpha discarded by encoder anyway)

## REVIEW PROTOCOL

You are a cold code reviewer. You have ZERO prior context about this project.

Read ALL files in your assignment. For each file:
1. Understand the purpose and API
2. Check every function for correctness

### ADVERSARIAL CHECK
For each public safe function, try to construct a safe-code call sequence that violates its documented invariant. Can you reach an invalid state through trait objects, generics, or creative composition? Pay special attention to:
- `unsafe impl Send for H264Encoder` — can two threads access it simultaneously?
- Buffer size calculations — can overflow produce undersized buffers?
- Network-received values (frame length, metadata, input events) — are all validated?

### CALLER-INVARIANT CHECK
For each public function, grep all callers. For each caller, identify what invariant the caller assumes about the return value. Does the callee actually guarantee that invariant in all code paths? For example:
- Does `capture_frame()` always return data matching `resolution()`?
- Does `receive_frame()` handle all error cases the caller expects?
- Does `decode()` return data sized correctly for `display_frame()`?

### CROSS-FILE EXPORT
List any invariant your files EXPORT that files outside your assignment depend on. Examples:
- "StreamMetadata::SIZE = 13 bytes" (server writes, client reads)
- "InputEvent::to_bytes() produces data that from_bytes() can parse" (client sends, server receives)
- "RGBA pixel data is width*height*4 bytes, tightly packed" (capture produces, encoder consumes)

## REPORTING

For each file, report one of:
  CLEAN: file — no issues found
  BUG: file:line [SEVERITY] — summary
    Evidence: what's wrong
    Impact: what happens
    Fix: suggested fix (1-2 lines)
  UNTESTED-SOFT: file:line — condition that would be worth testing
  UNTESTED-HARD: file:line — condition that can't be tested without hardware/compositor

End your review with a summary:
  N CRITICAL, N MODERATE, N LOW
  EXPORT: list of exported invariants from your files
